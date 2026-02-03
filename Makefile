# A simple makefile for running the server and web app

.PHONY: update
update:
	cargo update

# Infrastructure commands

.PHONY: tfvars
tfvars:
	@echo "# Generated from secrets.yaml - do not edit manually" > infra/terraform.tfvars
	@sops decrypt secrets.yaml | yq -r '"scw_application_id         = \"" + .cloud.scaleway.application_id + "\""' >> infra/terraform.tfvars
	@sops decrypt secrets.yaml | yq -r '"scw_application_access_key = \"" + .cloud.scaleway.access_key + "\""' >> infra/terraform.tfvars
	@sops decrypt secrets.yaml | yq -r '"scw_application_secret_key = \"" + .cloud.scaleway.secret_key + "\""' >> infra/terraform.tfvars
	@sops decrypt secrets.yaml | yq -r '"scw_organization_id        = \"" + .cloud.scaleway.organization_id + "\""' >> infra/terraform.tfvars
	@sops decrypt secrets.yaml | yq -r '"scw_project_id             = \"" + .cloud.scaleway.project_id + "\""' >> infra/terraform.tfvars
	@sops decrypt secrets.yaml | yq -r '"github_client_id           = \"" + .identity.github.prod.client_id + "\""' >> infra/terraform.tfvars
	@sops decrypt secrets.yaml | yq -r '"github_client_secret       = \"" + .identity.github.prod.client_secret + "\""' >> infra/terraform.tfvars
	@sops decrypt secrets.yaml | yq -r '"google_client_id           = \"" + .identity.google.client_id + "\""' >> infra/terraform.tfvars
	@sops decrypt secrets.yaml | yq -r '"google_client_secret       = \"" + .identity.google.client_secret + "\""' >> infra/terraform.tfvars
	@echo "Generated infra/terraform.tfvars from secrets.yaml"

.PHONY: backend-config
backend-config:
	@sops decrypt secrets.yaml | yq -r '"access_key = \"" + .cloud.scaleway.access_key + "\""' > infra/backend.hcl
	@sops decrypt secrets.yaml | yq -r '"secret_key = \"" + .cloud.scaleway.secret_key + "\""' >> infra/backend.hcl
	@echo "Generated infra/backend.hcl from secrets.yaml"

.PHONY: infra-up
infra-up: tfvars backend-config
	$(MAKE) -C infra init ARGS="-backend-config=backend.hcl"
	$(MAKE) -C infra apply ARGS="-auto-approve"
	@SDB_ID=$$(cd infra && tofu output -raw sdb_id); \
	SDB_ENDPOINT=$$(cd infra && tofu output -raw sdb_endpoint); \
	SDB_USERNAME=$$(cd infra && tofu output -raw sdb_username); \
	SDB_PASSWORD=$$(cd infra && tofu output -raw sdb_password); \
	DATABASE_URL=$$(echo "$$SDB_ENDPOINT" | sed "s|postgres://|postgres://$$SDB_USERNAME:$$SDB_PASSWORD@|"); \
	SCW_CONTAINER_ID=$$(cd infra && tofu output -raw container_id); \
	SCW_CONTAINER_URL=$$(cd infra && tofu output -raw container_url); \
	sops --set='["database"]["serverless_db"]["id"] "'"$$SDB_ID"'"' secrets.yaml; \
	sops --set='["database"]["serverless_db"]["endpoint"] "'"$$SDB_ENDPOINT"'"' secrets.yaml; \
	sops --set='["database"]["serverless_db"]["username"] "'"$$SDB_USERNAME"'"' secrets.yaml; \
	sops --set='["database"]["serverless_db"]["password"] "'"$$SDB_PASSWORD"'"' secrets.yaml; \
	sops --set='["database"]["database_url"] "'"$$DATABASE_URL"'"' secrets.yaml; \
	sops --set='["cloud"]["scaleway"]["container_id"] "'"$$SCW_CONTAINER_ID"'"' secrets.yaml; \
	sops --set='["cloud"]["scaleway"]["container_url"] "'"$$SCW_CONTAINER_URL"'"' secrets.yaml
	@echo "Infrastructure deployed. Secrets updated in secrets.yaml"

.PHONY: infra-down
infra-down: tfvars backend-config
	$(MAKE) -C infra init ARGS="-backend-config=backend.hcl"
	$(MAKE) -C infra destroy ARGS="-auto-approve"
	@echo "Infrastructure destroyed."

.PHONY: env
env:
	@sops decrypt secrets.yaml | yq -r '"DATABASE_URL=" + .database.database_url' > .env
	@sops decrypt secrets.yaml | yq -r '"SDB_ID=" + .database.serverless_db.id' >> .env
	@sops decrypt secrets.yaml | yq -r '"SDB_ENDPOINT=" + .database.serverless_db.endpoint' >> .env
	@sops decrypt secrets.yaml | yq -r '"SCW_CONTAINER_ID=" + .cloud.scaleway.container_id' >> .env
	@sops decrypt secrets.yaml | yq -r '"SCW_CONTAINER_URL=" + .cloud.scaleway.container_url' >> .env
	@sops decrypt secrets.yaml | yq -r '"GITHUB_CLIENT_ID=" + .identity.github.dev.client_id' >> .env
	@sops decrypt secrets.yaml | yq -r '"GITHUB_CLIENT_SECRET=" + .identity.github.dev.client_secret' >> .env
	@sops decrypt secrets.yaml | yq -r '"GITHUB_REDIRECT_URI=" + .identity.github.dev.redirect_uri' >> .env
	@sops decrypt secrets.yaml | yq -r '"GOOGLE_CLIENT_ID=" + .identity.google.client_id' >> .env
	@sops decrypt secrets.yaml | yq -r '"GOOGLE_CLIENT_SECRET=" + .identity.google.client_secret' >> .env
	@sops decrypt secrets.yaml | yq -r '"GOOGLE_REDIRECT_URI=" + .identity.google.dev.redirect_uri' >> .env
	@echo "Generated .env from secrets.yaml (dev)"

# Migrations commands

.PHONY: migrate-run
migrate-run:
	cd packages/api && cargo sqlx migrate run

.PHONY: migrate-revert
migrate-revert:
	cd packages/api && cargo sqlx migrate revert