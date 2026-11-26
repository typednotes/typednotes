# Typednotes TODO

## Sub-projects

Use the [illustrated system overview](docs/system-overview.md) to see how these
repositories fit together and where to file follow-up work.

- **[Typednotes app](docs/system-overview.md#app):** Web and Core in one app. Owns
  users, organizations, projects, notes and grants; adopts checked builds and
  persists execution state, schedules and the delivery outbox.
- **[Lode](docs/system-overview.md#lode):** Turns a note into both a formal Lean
  signature (including the effect row) and its implementation. Preserves
  caller-pinned contracts and platform bounds, tests the code, and publishes
  the Lean modules and `lun.json` when authorized.
- **[Lun](docs/system-overview.md#lun):** Checks the signature, implementation,
  effects and DAG wiring; builds a native driver and runs stateless steps with
  fresh authority. [Cross-build cell artifact reuse](docs/cell-packages.md) is a proposal.
- **[Linen](docs/system-overview.md#linen):** Linked Lean library for typed effects,
  capability evidence, reactive graphs and systems primitives. Not a service.
- **[Liaison](docs/system-overview.md#liaison):** Broker for delegated connector
  operations. Checks warrants and live permissions, uses provider credentials,
  and records outcomes. Hosts the native paired-messaging adapters.
- **[Secrets](docs/system-overview.md#secrets):** Authenticated vault with
  path-scoped access to credentials and policy records, backed by encrypted
  PostgreSQL storage.
- **[Ledger](docs/system-overview.md#ledger):** Owns the shared credit, hold and
  usage model and SQL tables, and runs the expired-hold sweeper.
- **[Infra](https://github.com/typednotes/infra):** Reusable Lean engine that
  plans and applies changes from typed infrastructure declarations.
- **[Typednotes Infra](https://github.com/typednotes/typednotes-infra):** Declares
  and deploys the Typednotes fleet, service access and migration histories.
- **[Typednotes Lite](https://github.com/typednotes/typednotes-lite):** Builds and
  publishes the combined Lite and Local images with matching service versions
  and migrations. Image publication is separate from deployment.

## App checklist

The completed items below record local implementation or verification, not
deployment or full CI coverage. See the [0.11.0 release notes](docs/release-0.11.0.md)
and [scheduling ownership](docs/scheduling-ownership.md) for the earlier rollout
baseline and its verification boundaries.

- [x] How is lode told it is fully in auto mode? Is it safe? When lode really need a user input, how can it ask the user for an input?
- [x] In https://app.typednotes.com/orgs/workspace-7827ce26-84e5-4ed9-b8a8-dfc1515560aa/projects/my-project/settings/repository?connected=github `Code connection` and `Account or organization` select menu overlap in some window width situations.
- [x] I'm not sure I understand the notion of workspace. Let's simplify and have only 3 levels: Org, Project and Notebook. (Superseded by the later Org → Project → Notes request.)
- [x] When a user connects for the first time he should be asked to link a github or gitlab repo and an AI account. If no org is present, the org could be created after the first repo's org and the first project should be named after the repo name. The user can skip this and in this case he can setup an org and project himself.
- [x] To simplify, let's ditch the notion of notebook. The user should be able to define nodes directly in the project and they cn be displayed directly in the project.
- [x] Selection of the model is weird, the selector menu should be aligned, the refresh button should be aligned too or maybe there is no need to refresh manually.
- [x] It is overly complicated to define permission at the node level. Permissions are at the project level for now, we'll see later for node level permission.
- [x] simplify the concepts and naming, the same thing should not have different names (Cell, Node), maybe it should be called a note. Some are source notes, some are sink notes, some are notes with inputs and an output.
- [x] When generating the code I got: Automatic code generation could not start: the code writer: timed out (Local fix verified; deployment of the matching writer/app is still required.)
- [x] Can you make sure the compilation of, lun, lode or other typednotes projects does not produce warning? lun or lode give: ```info: stderr:
/Users/nicolas.grislain/Typednotes/lun/.lake/packages/linen/ffi/tls.c:312:9: warning: 'SSL_set1_host' is deprecated [-Wdeprecated-declarations]
  312 |         SSL_set1_host(ssl, hostname);
      |         ^
/opt/homebrew/Cellar/openssl@4/4.0.3/include/openssl/ssl.h:1922:1: note: 'SSL_set1_host' has been explicitly marked deprecated here
 1922 | OSSL_DEPRECATEDIN_4_0 __owur int SSL_set1_host(SSL *s, const char *host);
      | ^
/opt/homebrew/Cellar/openssl@4/4.0.3/include/openssl/macros.h:205:31: note: expanded from macro 'OSSL_DEPRECATEDIN_4_0'
  205 | #define OSSL_DEPRECATEDIN_4_0 OSSL_DEPRECATED(4.0)
      |                               ^
/opt/homebrew/Cellar/openssl@4/4.0.3/include/openssl/macros.h:67:47: note: expanded from macro 'OSSL_DEPRECATED'
   67 | #define OSSL_DEPRECATED(since) __attribute__((deprecated))```
