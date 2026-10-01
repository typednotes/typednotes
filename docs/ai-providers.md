# AI providers and token costs

Organization settings → Connections includes every provider below. Paste its key,
or configure its named environment variable on the app deployment. Environment
keys can be imported only by an owner/admin of `TYPEDNOTES_AI_ENV_ORG` (set this to
the intended org slug); availability is reported without returning key values.
Keys go into the vault. Inference and classification go through the credential
broker with connection-scoped warrants.

Apply `migrations/0006_ai_providers.sql` and deploy the updated **lode** service
before using the expanded catalog. Lode now accepts the new provider warrants and
supports Messages, Chat Completions, Responses, Gemini and Radius's Pi messages.

## Providers

- Anthropic — `ANTHROPIC_API_KEY`
- Ant Ling — `ANT_LING_API_KEY`
- OpenAI — `OPENAI_API_KEY`
- DeepSeek — `DEEPSEEK_API_KEY`
- NVIDIA NIM — `NVIDIA_API_KEY`
- Google Gemini — `GEMINI_API_KEY`
- GitHub Copilot — `COPILOT_GITHUB_TOKEN`
- Mistral — `MISTRAL_API_KEY`
- Groq — `GROQ_API_KEY`
- Cerebras — `CEREBRAS_API_KEY`
- xAI — `XAI_API_KEY`
- OpenRouter — `OPENROUTER_API_KEY`
- Vercel AI Gateway — `AI_GATEWAY_API_KEY`
- ZAI Coding Plan (Global) — `ZAI_API_KEY`
- ZAI Coding Plan (China) — `ZAI_CODING_CN_API_KEY`
- OpenCode Zen and OpenCode Go (separate endpoints) — `OPENCODE_API_KEY`
- Radius — `RADIUS_API_KEY`
- TypeSafe classifiers — `TYPESAFE_API_KEY`
- Hugging Face — `HF_TOKEN`
- Fireworks — `FIREWORKS_API_KEY`
- Together AI — `TOGETHER_API_KEY`
- Baseten — `BASETEN_API_KEY`
- Kimi For Coding — `KIMI_API_KEY`
- Meta — `META_API_KEY`
- MiniMax — `MINIMAX_API_KEY`
- MiniMax (China) — `MINIMAX_CN_API_KEY`
- Moonshot AI (Global and China, separate endpoints) — `MOONSHOT_API_KEY`
- Qwen Token Plan and Individual (separate catalog entries) — `QWEN_TOKEN_PLAN_API_KEY`
- Qwen Token Plan (China) — `QWEN_TOKEN_PLAN_CN_API_KEY`
- Xiaomi MiMo — `XIAOMI_API_KEY`
- Xiaomi MiMo Token Plan (China) — `XIAOMI_TOKEN_PLAN_CN_API_KEY`
- Xiaomi MiMo Token Plan (Amsterdam) — `XIAOMI_TOKEN_PLAN_AMS_API_KEY`
- Xiaomi MiMo Token Plan (Singapore) — `XIAOMI_TOKEN_PLAN_SGP_API_KEY`
- Scaleway — `SCW_SECRET_KEY`
- Other OpenAI-compatible servers — explicit base URL and key.

The checked endpoint, protocol, plan flag and environment name live together in
`packages/api/src/ai.rs`. Regional providers remain distinct vault/provider ids.
An optional endpoint override supports proxies, private NIM instances and dedicated
Scaleway deployments. For a non-default Scaleway project use
`https://api.scaleway.ai/PROJECT_UUID/v1`; the default is
`https://api.scaleway.ai/v1`.

Copilot requires a GitHub token/account with Copilot Chat entitlement. It uses the
individual Copilot inference API with request-initiator headers. A generic GitHub
PAT with no Copilot access is not a substitute. OpenCode/Copilot gateways choose
their native API according to the model family; use the exact provider model id.

## Typed classification

TypeSafe is a decision API, not a code-writing chat model. Its connection row
includes a yes/no evaluation workbench. `classify_with_ai` also accepts the full
native `state`, `model`, `questions` request, including Choice, Score and Noul.
Classification uses a budgeted broker call, with the deployment's model-call
credit cost. Responses retain typed answers, confidence/probabilities and usage.

## Pricing UI

The connection's **Models and pricing** panel, notebook model picker and TypeSafe
workbench load model IDs from that account's native `models.list` operation via
Liaison. No API key is exposed and no inference request is made to discover
models. Menus normalize Gemini's `models/` prefix, deduplicate and sort IDs,
preserving provider namespaces. Refresh explicitly reloads the provider catalog.
The broker adapter uses the provider's inventory endpoint/default page; a model
absent from that response is not invented from a public pricing catalog.

Connect an account before choosing its model. Unavailable catalogs display the
provider/broker error with a retry control. An existing saved model absent from
the current catalog is identified without silently replacing it. Organization
Connector/provider grants and connection `models.list` permission must be enabled;
the model's inference grant is still checked separately when it runs.

The connection panel and notebook's model selector display **USD per million
tokens** for the exact model id: input, output and available cache rates. A token
calculator shows an uncached-request estimate. The implementation log retains the
provider's token usage. Cached input is normalized separately by the updated writer.

- Most public rates come from [Models.dev](https://models.dev), cached for one hour.
- Radius publishes rates in its [gateway configuration](https://radius.pi.dev/v1/config).
- The documented Jev 1.13/aliases rate is $0.042 per million input tokens, with free
  output ([TypeSafe models](https://docs.typesafe.ai/models), checked 2026-09-30).
- Unknown models/rates show **Unavailable**, not $0.
- Subscription/coding/token-plan providers are labeled as plan-based. Zero catalog
  rates are not presented as unlimited free usage, and no dollar estimate is made
  for those plans. Quotas, multipliers and monthly charges must be checked with the provider.

These are public list-price estimates, not invoices. Discounts, routing, overrides,
long-context surcharges, subscriptions and provider billing can differ. Typednotes
credits are shown/accounted separately; the estimate does not convert credits to dollars.

## Contract sources

- Provider endpoints/auth mappings: [pi-ai provider definitions](https://github.com/earendil-works/pi/tree/main/packages/ai/src/providers).
- Regional/model metadata: [Models.dev API](https://models.dev/api.json).
- [TypeSafe API](https://docs.typesafe.ai/api).
- [Scaleway Generative APIs](https://www.scaleway.com/en/docs/generative-apis/).
- [OpenCode model endpoints](https://opencode.ai/v2/docs/console/models).

Coverage checks include every requested environment name, regional endpoint
separation, catalog/database agreement, native request/response/tool replay and
cost arithmetic. Live inference requires the account's real credentials and model
entitlements; local checks use fixtures rather than making billed calls.
