# Reference vectors

What atproto's reference implementations make of fixed keys, for the parts
the interop vectors leave out. `identity.json` is from `identity.mjs`, run
against @did-plc/lib 0.0.4 and the space alpha's @atproto/crypto and
@atproto/xrpc-server (0.0.0-spaces-alpha-20261022141228): PLC operations
and the DIDs they make, the DID document the directory serves, and a
service-auth JWT. `knotless.json` is a service-auth JWT knotless's own
client signs (`knotless::crypto::P256Key::sign_jwt`), checked here as
knotless checks one.

`space-vectors.json` is pds.js's (`test/fixtures/space-vectors.json`),
made by running `@atproto/space` (its `generatedFrom` names the build): the
LtHash set hash, a space repo's commit, its MAC and MAC header and its STAR
archive, a record's MAC, a KEM decapsulation and its want-MAC header, and
an HTTP signature, which the credentials' pull reads.

