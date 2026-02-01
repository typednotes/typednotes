# A simple makefile for running the server and web app

.PHONY: update
update:
	cargo update

# Infrastructure commands
.PHONY: infra-init
infra-init:
	$(MAKE) -C infra init

.PHONY: infra-plan
infra-plan:
	$(MAKE) -C infra plan

.PHONY: infra-apply
infra-apply:
	$(MAKE) -C infra apply

.PHONY: infra-destroy
infra-destroy:
	$(MAKE) -C infra destroy

.PHONY: infra-output
infra-output:
	$(MAKE) -C infra output