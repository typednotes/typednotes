-- AI provider ids match the catalog, vault paths and warrant capabilities.
-- No credentials or deployment environment values are stored in Postgres.
alter table connections drop constraint connections_provider_check;
alter table connections add constraint connections_provider_check check (provider in
    ('github','gitlab','gdrive','google-calendar','microsoft-calendar','caldav',
     'gmail','outlook','jmap','notion','dropbox','s3','azure','slack','whatsapp','signal',
     'anthropic','mistral','openai','openai-compatible','ant-ling','baseten','cerebras',
     'deepseek','fireworks','github-copilot','gemini','groq','huggingface','kimi-coding',
     'meta','minimax','minimax-cn','moonshotai','moonshotai-cn','nvidia','opencode-go',
     'opencode','openrouter','qwen-token-plan','qwen-token-plan-cn',
     'qwen-token-plan-individual','radius','scaleway','together','typesafe',
     'vercel-ai-gateway','xiaomi','xiaomi-token-plan-ams','xiaomi-token-plan-cn',
     'xiaomi-token-plan-sgp','zai-coding-cn','zai','xai'));
