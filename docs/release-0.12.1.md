# Typednotes v0.12.1 — implemented architecture and diagrams

Documentation patch on the v0.12.0 native paired-messaging implementation. Runtime
behavior, service APIs, dependency pins and migration requirements are unchanged.

- Add [the implemented system architecture](system-architecture.md), tracing
  component responsibilities and execution/authority/storage boundaries to their
  source owners, with references to related systems and academic papers.
- Include five accessible, editable SVG diagrams for the logical system,
  generation/check/adoption loop, durable execution/outbox recovery, authority
  intersection and Full/Lite/Local deployment shapes.
- Correct service summaries and mark historical/proposed IdP, core and generic
  agent designs explicitly. Document the combined-image native-adapter packaging
  gap instead of implying every deployment shape supports paired messaging.
- Require code, service documentation and SVG diagrams to remain synchronized in
  `AGENTS.md`; link the new overview from the root README and service index.

Local documentation checks passed: 95 local/source/heading links, SVG XML,
accessibility titles/descriptions, rendered text fit and label-collision checks.
This patch does not add new live-provider or deployment verification evidence.

Deploying the included paired-messaging implementation still requires matching
**Liaison v0.7.0**, app migration **0016** and configured persistent encrypted device
storage. See [v0.12.0](release-0.12.0.md) and [paired messaging](paired-messaging.md).

Publish the intended latest main commit and its v0.12.1 tag together, then verify
exact-commit main CI before image promotion. Earlier unpublished local tags should
not be assumed to have their own main-CI evidence; do not move existing tags to
another commit. Source/tag publication is separate from cloud deployment.
