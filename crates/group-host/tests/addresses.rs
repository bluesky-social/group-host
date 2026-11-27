//! An account made before addresses, given its handle as its address.

mod common;

#[test]
fn gives_an_account_made_before_addresses_its_handle_as_its_address() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("host.sqlite");
    let store = group_host::store::Store::open(&path).unwrap();
    let account = group_host::store::Account {
        did: "did:plc:a".into(),
        handle: "a.groups.test".into(),
        signing: atproto::Keypair::generate(atproto::Curve::K256),
        kem: atproto::Keypair::generate(atproto::Curve::P256),
    };
    let tree = atproto::mst::Tree::build(&Default::default());
    let commit =
        atproto::Commit::sign(&account.did, tree.root(), "3l5mz5mkbn22b", &account.signing);
    let head = group_host::store::Head {
        cid: commit.cid(),
        rev: commit.rev.clone(),
        block: commit.to_bytes(),
    };
    store.create_account(&account, &head, 0, &[]).unwrap();
    drop(store);
    // As a database from before: no address, its schema a version back.
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("DELETE FROM address", []).unwrap();
    // Back to just before the backfill.
    let backfill = group_host::store::MIGRATIONS
        .iter()
        .position(|m| m.contains("INSERT OR IGNORE INTO address"))
        .unwrap();
    common::rewind(&conn, backfill);
    drop(conn);
    let store = group_host::store::Store::open(&path).unwrap();
    assert_eq!(
        store.address("did:plc:a").unwrap().as_deref(),
        Some("a.groups.test")
    );
}
