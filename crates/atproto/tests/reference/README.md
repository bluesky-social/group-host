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

`space-meta.json` is from `space-meta.mjs`, run against `@atproto/space`
0.0.0-spaces-alpha-20261029171913, the first alpha with a space meta
archive (exportSpaceMeta's): two archives it writes and what it reads
back, and archives it refuses (its own tests' cases, built as they build
them), each with its refusal. That build's published dependency ranges
resolve `@atproto/lex-cbor` and `@atproto/lex-data` to a stray `0.0.0`
whose own dependencies are `workspace:*`, so install it with each of
`@atproto/{car,crypto,syntax,lex-cbor,lex-data}` pinned to the same
version in `overrides`.


`jose.json` is from `jose.mjs`, run against jose 5.10.0 (panva's, which
@atproto/oauth-provider checks DPoP proofs and client assertions with):
fixed P-256 and secp256k1 keys' public JWKs and RFC 7638 thumbprints, a
DPoP proof each signs, and RFC 9449's own example key and thumbprint.

`lexicon.json` is from `lexicon.mjs`, run against @atproto/lexicon 0.7.18
(installed on its own, in a directory of its own) over a checkout of
lexicon.js (github.com/bigmoves/lexicon.js), whose data, format and
end-to-end cases it reads: each case's lexicons, the def it is checked
against, the value, and the reference's verdict, with a few cases of this
crate's own. `tests/lexicon.rs` holds where `atproto::lexicon` departs
from it, and its module says why.
