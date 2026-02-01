# A simple makefile for running the server and web app

.PHONY: update
update:
	cargo update

# Infrastructure commands

.PHONY: infra-up
infra-up:
	$(MAKE) -C infra init
	$(MAKE) -C infra apply ARGS="-auto-approve"
	@SDB_ENDPOINT=$$(cd infra && tofu output -raw sdb_endpoint); \
	SDB_ID=$$(cd infra && tofu output -raw sdb_id); \
	SDB_USERNAME=$$(cd infra && tofu output -raw sdb_username); \
	SDB_PASSWORD=$$(cd infra && tofu output -raw sdb_password); \
	DATABASE_URL=$$(echo "$$SDB_ENDPOINT" | sed "s|postgres://|postgres://$$SDB_USERNAME:$$SDB_PASSWORD@|"); \
	SCW_REGISTRY_ENDPOINT=$$(cd infra && tofu output -raw registry_endpoint); \
	SCW_CONTAINER_URL=$$(cd infra && tofu output -raw container_url); \
	SCW_CONTAINER_ID=$$(cd infra && tofu output -raw container_id); \
	for var in SDB_ID SDB_ENDPOINT SDB_USERNAME SDB_PASSWORD DATABASE_URL SCW_REGISTRY_ENDPOINT SCW_CONTAINER_URL SCW_CONTAINER_ID; do \
		val=$$(eval echo \$$$$var); \
		if grep -q "^$$var=" .env 2>/dev/null; then \
			sed -i '' "s|^$$var=.*|$$var=$$val|" .env; \
		else \
			echo "$$var=$$val" >> .env; \
		fi; \
	done
	@echo "Infrastructure deployed. Outputs saved to .env"

.PHONY: infra-down
infra-down:
	$(MAKE) -C infra destroy ARGS="-auto-approve"
	@echo "Infrastructure destroyed."

# Migrations commands

.PHONY: migrate-run
migrate-run:
	cd packages/api && cargo sqlx migrate run

.PHONY: migrate-revert
migrate-revert:
	cd packages/api && cargo sqlx migrate revert