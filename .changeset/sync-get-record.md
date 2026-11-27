---
default: patch
---

# A group's record is served with its proof

The host answers `com.atproto.sync.getRecord`, as a PDS does: a CAR rooted at the group's signed commit, holding the tree's nodes down to the record and the record itself, or proving it absent. Apps that read a record a group just wrote through `getRecord` were refused with `MethodNotImplemented`, so the record showed only once the network brought it.
