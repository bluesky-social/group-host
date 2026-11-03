//! CIDs, as atproto blesses them.

use atproto::cid::{Cid, CidError, DAG_CBOR, RAW};

#[test]
fn of_cbor_and_raw() {
    let record = Cid::of_cbor(b"\xa0");
    assert_eq!(record.codec(), DAG_CBOR);
    assert!(record.to_string().starts_with("bafyrei"));
    let blob = Cid::of_raw(b"hello");
    assert_eq!(blob.codec(), RAW);
    assert!(blob.to_string().starts_with("bafkrei"));
    assert_ne!(record.digest(), blob.digest());
    assert_eq!(format!("{blob:?}"), format!("Cid({blob})"));
}

#[test]
fn round_trips() {
    let text = "bafyreidfayvfuwqa7qlnopdjiqrxzs6blmoeu4rujcjtnci5beludirz2a";
    let cid = Cid::parse(text).unwrap();
    assert_eq!(cid.to_string(), text);
    assert_eq!(Cid::from_bytes(&cid.to_bytes()).unwrap(), cid);
}

#[test]
fn refuses_what_is_not_blessed() {
    assert_eq!(Cid::parse("zQm"), Err(CidError::Encoding));
    assert_eq!(Cid::parse("b!!"), Err(CidError::Encoding));
    // Base32 whose last character leaves bits that are not zero.
    assert_eq!(Cid::parse("bb"), Err(CidError::Encoding));
    // A CIDv0, and one of another codec.
    assert_eq!(Cid::from_bytes(&[0x12, 0x20]), Err(CidError::Unblessed));
    let mut dag_pb = Cid::of_raw(b"").to_bytes();
    dag_pb[1] = 0x70;
    assert_eq!(Cid::from_bytes(&dag_pb), Err(CidError::Unblessed));
    assert_eq!(Cid::from_bytes(&dag_pb[..20]), Err(CidError::Unblessed));
    assert_eq!(
        CidError::Encoding.to_string(),
        "a CID is base32, starting with b"
    );
    assert_eq!(
        CidError::Unblessed.to_string(),
        "a CID is version 1 and SHA-256, of DAG-CBOR or raw"
    );
}
