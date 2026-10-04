<!-- Implemented and locally verified against documented Lun 0.4.1 / Linen 1.12.0.
     App 0.11.0, Lode 0.5.0 and fleet 0.7.0 have local release commits/tags;
     publication and deployment remain pending.
     See docs/release-0.11.0.md and docs/scheduling-ownership.md for verification,
     trusted boundaries and the required coordinated migration/rollout. Checkmarks
     denote local implementation/verification, not deployment or every CI platform. -->

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
