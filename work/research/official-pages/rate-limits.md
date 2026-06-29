> ## Documentation Index
> Fetch the complete documentation index at: https://exa.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Rate Limits

> Default rate limits for Exa API endpoints

***

<Info>
  Need higher rate limits? Contact us at [sales@exa.ai](mailto:sales@exa.ai) to discuss an Enterprise plan.
</Info>

Our API endpoints have default rate limits to ensure reliable performance for all users. Most endpoints are limited by QPS. The legacy Research API (`/research/v1`) uses concurrent task limits for its long-running operations.

| Endpoint                           | Limit               |
| ---------------------------------- | ------------------- |
| `/search`                          | 10 QPS\*            |
| `/contents`                        | 100 QPS             |
| `/answer`                          | 10 QPS              |
| `/research/v1` (legacy/deprecated) | 15 concurrent tasks |

*\*QPS = Queries Per Second*
