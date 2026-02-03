# Infrastructure management with OpenTofu

.PHONY: init
init:
	tofu init $(ARGS)

.PHONY: plan
plan:
	tofu plan

.PHONY: apply
apply:
	tofu apply $(ARGS)

.PHONY: destroy
destroy:
	tofu destroy $(ARGS)

.PHONY: fmt
fmt:
	tofu fmt

.PHONY: validate
validate:
	tofu validate

.PHONY: output
output:
	tofu output

.PHONY: refresh
refresh:
	tofu refresh

.PHONY: clean
clean:
	rm -rf .terraform
	rm -f .terraform.lock.hcl
