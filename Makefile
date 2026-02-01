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
	for var in SDB_ENDPOINT SDB_ID; do \
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