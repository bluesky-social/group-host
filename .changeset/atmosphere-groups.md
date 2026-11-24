---
default: minor
---

# The project is Atmosphere Groups

The lexicons are `org.atgrp.*`, and the host's own methods `org.atgrp.internal.*`; they were `fyi.opensocial.*`.

The command is `atgrp`, and reads `ATGRP_AS`, `ATGRP_PASSWORD`, `ATGRP_HOST`, `ATGRP_HOST_DID`, `ATGRP_SEED_INVITE` and `ATGRP_IMPORT_PASSWORD`. `just atgrp` runs it.

The host's DID document names its service `#atgrp`, of type `AtgrpGroupHost`, so an app proxies to `<did>#atgrp`.

The project is Atmosphere Groups, implementing the Atmosphere Groups proposal.
