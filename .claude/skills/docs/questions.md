# The questions each design page answers

In roughly the order a reader asks them. Start a page's context from its
list, and add what the facts raise. A page with no list here gets one
written the same way: what the thing is, what problems shape it, then
one question per mechanism.

## README.md (the overview)

1. What is a group host, and what does it do for a group that a PDS does
   for a person?
2. What are its pieces: the server, the console, the command, the MCP
   server, the atproto primitives, the lexicons?
3. Where does each kind of data live: the group's repo, its spaces, the
   members' own repos, the host's database?
4. Which design page answers what?
5. What is built, and what comes next?

## spaces.md

1. What is a space, and what does a group keep in one?
2. Who writes where: the group's own repo in a space, and each member's?
3. How does the host act as a space's authority: who may read, who may
   write, how credentials are given and revoked?
4. How does it learn of writes, and keep an index of what is in a space?
5. How does it read the records and files of members on other PDSes, and
   whom does it trust for their identities?
6. How does a page or an app read a space through the host?
7. What spaces does a group start with (meta and members), and what is
   in each? How are spaces created and deleted?
8. What is the access record, and how does it decide who reads and
   writes?
9. What happens when a space or a group is deleted, and what stays on
   members' PDSes?
10. When are credentials revoked, and how long does one last?
11. What does the host do in spaces it is not the authority for: invites
    in a person's inbox, and checkUserAccess?
12. Who may see a space's images, through /img and getSpaceBlob?
13. What happens to a group's spaces while it is switched off?

## groups.md

1. What is a group, as an account, and what does starting one make?
2. What decides who may do what in it?
3. What may someone outside, or inside, see of it?
4. How are its profile, rules and pictures changed?
5. What other spaces can it have, and who decides who reads them?
6. How does it get a handle on a domain of its own?
7. How is it switched off, on, and deleted, and what is left after?
8. How do people find a group: its declaration, listGroups,
   getActorGroups, and what a hidden group still shows?
9. What is a group's address, as distinct from its handle, and how is a
   free one found?
10. What does the host keep of a group outside its records?
11. What becomes of a deleted group's DID when a steward holds a
    recovery key?

## membership.md

1. What is a role, and what does it grant?
2. What makes someone a member, and where is that recorded?
3. How does someone join: open, by approval, by invite?
4. How are invites delivered, seen, declined and revoked?
5. What happens when a member leaves or is removed, to their access and
   to what they connected?
6. Where is membership recorded (the members space: membership, role and
   permissions records), and what does each hold?
7. What does removing someone take with them: credentials, the OAuth
   grants they approved, their MCP clients, their app passwords and
   sessions?
8. How long does the host keep trying to deliver an invite to someone's
   inbox?
9. What do the default roles do, on joining and on admitting?

## signing-in.md

1. Who signs in to what: a person to the console, an app as a group, an
   MCP client as a member?
2. How does the console sign people in, and what does it keep?
3. How does an app sign in as a group, when a group has no password?
4. Who approves, and what does the app then get?
5. What may a token do, and how is that checked on each call?
6. How are tokens listed and revoked, by the group and by its members?
7. What are app passwords for, and what may they not do?
8. How does an app start a group and sign in to it at once
   (provisioning)?
9. How does an app's own session differ from a person's service auth
   when calling the host's methods (cookie vs service auth vs group
   token), and which methods take which?
10. What does a grant's lifetime look like end to end: request (5 min),
    code (60 s), access (1 h / 2 h / 15 min), grant (14 / 90 days,
    sliding), and when is each forgotten?
11. What is the group's activity log, who sees it, and what does (and
    doesn't) it record?
12. What happens to an app's tokens when the group is switched off
    (tokens still refresh; writes refused; getSession says
    `deactivated`)?
13. How do MCP grants relate to group grants: separate server,
    separate table, not in `listGroupTokens` — where does the reader go
    to see them?
14. What may a confidential client vs a public client do differently
    (lifetime; same-key rule; native clients must be public)?
15. How does an app find this host's authorization server for a group
    (resource metadata at PUBLIC_URL; the group's DID document naming
    this PDS)?
16. Why are nonces stateless and tokens opaque-but-stored (the trade-off
    between checking without storage and revoking at once)?

## moderation.md

1. Who moderates a group, and with what permissions?
2. What is a label, where is it written, and what does it do?
3. What is in the queue, and how is a subject resolved or escalated?
4. Who may report, and what becomes of a report?
5. What does a label do once written: which values the host acts on, and
   which are left to apps?
6. How does a role get moderation permissions, and what do the roles a
   group starts with hold?
7. What happens to the queue when a group is deleted?

## mcp.md

1. What is an MCP client, and what may it do in a group? Which
   command is each tool the twin of?
2. How does it connect and sign in, and how is that kept apart from apps
   signing in as a group?
3. What are its tools, and how is each call decided?
4. How does a member see and disconnect one?
5. How long do an MCP client's tokens last, and what ends a connection?

## recovery.md

1. What could go wrong with a group's identity if the host failed or
   misbehaved?
2. What is a recovery key, and what may its holder do that the host
   cannot undo?
3. How are keys added, listed and removed?
4. What happens to recovery keys when a group moves to another host, and
   when it is deleted?

## moving.md

1. Why would a group move to another host, and what has to move with it?
2. What are the steps, on each host, and what makes each safe?
3. How does an existing account become a group here, and what of it
   comes along: its repo, its handle, its spaces?
4. How is that done from the console, and from the command?
5. What happens when the host itself changes its address?
6. What must still be true for a move to start (the host still holds a
   rotation key)?
7. What does the new host do once the group is switched on: telling
   relays, reading each space again for the index?
8. What becomes of the group on the host it left?
9. What if the group's address is taken on the new host?
10. Which moves can be done from the console, and which only with the
    command?
