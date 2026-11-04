# group-host

A group host for atproto, in Rust: the PDS, space authority and OAuth
server for groups, and the console where people start and run them. It
implements the [opensocial proposal](https://tangled.org/opensocial.group/proposal).

```sh
just setup
just serve      # the host, on :2600
just web-dev    # the console
```

[docs/design](docs/design/README.md) says how it is put together and what
is still to come; [docs/development.md](docs/development.md) how to
build and test it. AGENTS.md is for agents working here.

Licensed under MIT or Apache-2.0, at your option.
