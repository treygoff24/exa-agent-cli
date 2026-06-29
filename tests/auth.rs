use exa_agent_cli::auth::{
    not_authenticated_error, resolve_api_credential, resolve_credential, resolve_profile,
    resolve_service_credential, CredentialInput, CredentialNamespace, CredentialSource, Keyring,
    KeyringError, NoopKeyring, Secret,
};
use std::collections::HashMap;

#[derive(Default)]
struct FakeKeyring {
    values: HashMap<String, String>,
    fail: bool,
}

impl FakeKeyring {
    fn with(mut self, service: &str, value: &str) -> Self {
        self.values.insert(service.to_string(), value.to_string());
        self
    }
}

impl Keyring for FakeKeyring {
    fn get(&self, service: &str) -> Result<Option<String>, KeyringError> {
        if self.fail {
            Err(KeyringError)
        } else {
            Ok(self.values.get(service).cloned())
        }
    }
}

#[test]
fn secret_display_debug_and_status_never_leak_value() {
    let secret = Secret::new("test-api-key-123456").unwrap();
    let display = secret.to_string();
    let debug = format!("{secret:?}");
    let json = serde_json::to_string(&secret).unwrap();

    assert!(!display.contains("test-api-key"));
    assert!(!debug.contains("test-api-key"));
    assert!(!json.contains("test-api-key"));
    assert!(display.contains("3456"));
    assert!(display.contains("fp_"));
    assert_eq!(Secret::new("abc").unwrap().last4(), "<short>");
    assert!(!Secret::new("abc").unwrap().to_string().contains("abc"));

    let resolved = resolve_api_credential(
        &CredentialInput {
            explicit: Some("test-api-key-123456".to_string()),
            ..CredentialInput::default()
        },
        &NoopKeyring,
    )
    .unwrap();
    let status = serde_json::to_string(&resolved.status()).unwrap();
    assert!(!status.contains("test-api-key"));
    assert!(status.contains("\"redacted\":true"));
    assert!(status.contains("\"last4\":\"3456\""));
}

#[test]
fn source_precedence_is_flag_then_stdin_then_env_then_keyring() {
    let keyring = FakeKeyring::default().with("exa-agent:api:default", "keyring-api-4444");

    let from_flag = resolve_api_credential(
        &CredentialInput {
            explicit: Some("flag-api-1111".to_string()),
            stdin: Some("stdin-api-2222".to_string()),
            env: Some("env-api-3333".to_string()),
            ..CredentialInput::default()
        },
        &keyring,
    )
    .unwrap();
    assert_eq!(from_flag.source, CredentialSource::Explicit);
    assert_eq!(from_flag.secret.last4(), "1111");

    let from_stdin = resolve_api_credential(
        &CredentialInput {
            stdin: Some("stdin-api-2222".to_string()),
            env: Some("env-api-3333".to_string()),
            ..CredentialInput::default()
        },
        &keyring,
    )
    .unwrap();
    assert_eq!(from_stdin.source, CredentialSource::Stdin);
    assert_eq!(from_stdin.secret.last4(), "2222");

    let from_env = resolve_api_credential(
        &CredentialInput {
            env: Some("env-api-3333".to_string()),
            credential_file: Some("file-api-4444".to_string()),
            ..CredentialInput::default()
        },
        &keyring,
    )
    .unwrap();
    assert_eq!(from_env.source, CredentialSource::Env("EXA_API_KEY"));
    assert_eq!(from_env.secret.last4(), "3333");

    let from_file = resolve_api_credential(
        &CredentialInput {
            credential_file: Some("file-api-4444".to_string()),
            credential_file_path: Some("/tmp/credentials.json".to_string()),
            ..CredentialInput::default()
        },
        &keyring,
    )
    .unwrap();
    assert_eq!(
        from_file.source,
        CredentialSource::CredentialFile {
            path: "/tmp/credentials.json".to_string()
        }
    );
    assert_eq!(from_file.secret.last4(), "4444");

    let from_keyring = resolve_api_credential(&CredentialInput::default(), &keyring).unwrap();
    assert_eq!(
        from_keyring.source,
        CredentialSource::Keyring {
            service: "exa-agent:api:default".to_string()
        }
    );
    assert_eq!(from_keyring.secret.last4(), "4444");
}

#[test]
fn api_and_service_scopes_use_distinct_env_and_keyring_namespaces() {
    let keyring = FakeKeyring::default()
        .with("exa-agent:api:work", "api-store-1111")
        .with("exa-agent:service:work", "service-store-2222");
    let input = CredentialInput {
        profile: Some("work".to_string()),
        ..CredentialInput::default()
    };

    let api = resolve_api_credential(&input, &keyring).unwrap();
    let service = resolve_service_credential(&input, &keyring).unwrap();

    assert_eq!(api.namespace, CredentialNamespace::Api);
    assert_eq!(service.namespace, CredentialNamespace::Service);
    assert_eq!(api.secret.last4(), "1111");
    assert_eq!(service.secret.last4(), "2222");

    let api_env = resolve_credential(
        CredentialNamespace::Api,
        &CredentialInput {
            env: Some("api-env-3333".to_string()),
            ..input.clone()
        },
        &NoopKeyring,
    )
    .unwrap();
    let service_env = resolve_credential(
        CredentialNamespace::Service,
        &CredentialInput {
            env: Some("service-env-4444".to_string()),
            ..input
        },
        &NoopKeyring,
    )
    .unwrap();
    assert_eq!(api_env.source, CredentialSource::Env("EXA_API_KEY"));
    assert_eq!(service_env.source, CredentialSource::Env("EXA_SERVICE_KEY"));
}

#[test]
fn empty_credentials_and_keyring_errors_are_ignored() {
    let keyring = FakeKeyring::default().with("exa-agent:api:default", "keyring-api-7777");
    let resolved = resolve_api_credential(
        &CredentialInput {
            explicit: Some(" ".to_string()),
            stdin: Some("\n".to_string()),
            env: Some("\t".to_string()),
            ..CredentialInput::default()
        },
        &keyring,
    )
    .unwrap();
    assert_eq!(resolved.secret.last4(), "7777");

    let missing = resolve_api_credential(
        &CredentialInput {
            env: Some(" ".to_string()),
            ..CredentialInput::default()
        },
        &FakeKeyring {
            fail: true,
            ..FakeKeyring::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        missing.checked,
        vec![
            "--api-key".to_string(),
            "--api-key-stdin".to_string(),
            "EXA_API_KEY".to_string(),
            format!(
                "file:{}:api_key",
                exa_agent_cli::auth::credentials_path().display()
            ),
            "keyring:exa-agent:api:default".to_string()
        ]
    );
}

#[test]
fn noop_keyring_falls_through_to_not_authenticated_with_checked_rungs() {
    let missing =
        resolve_service_credential(&CredentialInput::default(), &NoopKeyring).unwrap_err();
    assert_eq!(missing.profile, "default");
    assert_eq!(
        missing.checked,
        vec![
            "--service-key".to_string(),
            "--service-key-stdin".to_string(),
            "EXA_SERVICE_KEY".to_string(),
            format!(
                "file:{}:service_key",
                exa_agent_cli::auth::credentials_path().display()
            ),
            "keyring:exa-agent:service:default".to_string()
        ]
    );

    let err = not_authenticated_error(&missing);
    assert_eq!(err.category(), 2);
    assert_eq!(err.diag().code, "not_authenticated");
    assert_eq!(
        err.diag().details.as_ref().unwrap()["checked"],
        serde_json::json!(missing.checked)
    );
    assert_eq!(missing.checked[2], "EXA_SERVICE_KEY");
}

#[test]
fn profile_prefers_flag_then_env_then_default() {
    assert_eq!(resolve_profile(Some("work"), Some("env")), "work");
    assert_eq!(resolve_profile(Some(" "), Some("env")), "env");
    assert_eq!(resolve_profile(None, Some(" ")), "default");
}
