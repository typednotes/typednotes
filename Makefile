# A simple makefile for running the server and web app

.PHONY: update
update:
	cargo update

# Infrastructure commands

# Helper to update or add a variable in .env
# Usage: $(call set_env,VAR_NAME,value)
define set_env
	@if grep -q "^$(1)=" .env 2>/dev/null; then \
		sed -i '' 's|^$(1)=.*|$(1)=$(2)|' .env; \
	else \
		echo "$(1)=$(2)" >> .env; \
	fi
endef

.PHONY: infra-up
infra-up:
	$(MAKE) -C infra init
	$(MAKE) -C infra apply ARGS="-auto-approve"
	$(call set_env,SDB_ENDPOINT,$$(cd infra && tofu output -raw sdb_endpoint))
	$(call set_env,SDB_ID,$$(cd infra && tofu output -raw sdb_id))
	@echo "Infrastructure deployed. Outputs saved to .env"

.PHONY: infra-down
infra-down:
	$(MAKE) -C infra destroy ARGS="-auto-approve"
	@echo "Infrastructure destroyed."