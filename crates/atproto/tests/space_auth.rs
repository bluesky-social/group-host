//! Space tokens and signed requests: the reference's signed request
//! (tests/reference/space-vectors.json), and each token's rules.

use atproto::keys::{Curve, Keypair};
use atproto::space_auth::{
    CREDENTIAL_MAX_AGE, Expect, Kind, Make, create, parse, sign_request, space_host_aud, verify,
    verify_request,
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use std::collections::HashMap;

const NOW: u64 = 1_793_736_300;
const AUTHORITY: &str = "did:plc:authority";
const SPACE: &str = "at://did:plc:authority/space/fyi.opensocial.members/self";

fn vectors() -> Value {
    let path = format!(
        "{}/tests/reference/space-vectors.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn reader<'a>(headers: &'a HashMap<String, String>) -> impl Fn(&str) -> Option<&'a str> + 'a {
    move |k| headers.get(k).map(String::as_str)
}

#[test]
fn checks_a_request_the_reference_signed() {
    let v = vectors();
    let s = &v["httpSignature"];
    let headers: HashMap<String, String> = s["headers"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();
    verify_request(reader(&headers), s["keyId"].as_str().unwrap()).unwrap();
    let mut changed = headers.clone();
    changed.insert("atproto-space-audience".into(), "did:plc:cccc".into());
    let refused = verify_request(reader(&changed), s["keyId"].as_str().unwrap()).unwrap_err();
    assert_eq!(refused.message, "invalid HTTP message signature");
}

fn signed(key: &Keypair) -> HashMap<String, String> {
    let headers = sign_request(key, "Atproto-Space-Credential a.b.c", AUTHORITY).unwrap();
    headers
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

#[test]
fn signs_requests_and_refuses_every_other_form() {
    let key = Keypair::generate(Curve::P256);
    let id = key.public().did_key();
    let headers = signed(&key);
    verify_request(reader(&headers), &id).unwrap();
    assert_eq!(
        sign_request(&Keypair::generate(Curve::K256), "a", "b")
            .unwrap_err()
            .message,
        "signature key must be a P-256 did:key"
    );
    let input = headers["signature-input"].clone();
    let with = |k: &str, v: Option<&str>| {
        let mut h = headers.clone();
        match v {
            Some(v) => h.insert(k.into(), v.into()),
            None => h.remove(k),
        };
        verify_request(reader(&h), &id).unwrap_err().message
    };
    assert_eq!(
        with("authorization", None),
        "missing the authorization or atproto-space-audience header"
    );
    assert_eq!(
        with("signature", None),
        "missing or malformed signature headers"
    );
    assert_eq!(
        with("signature", Some("other=:AA==:")),
        "missing or malformed signature headers"
    );
    assert_eq!(
        with(
            "signature-input",
            Some("atproto-space-credential=(\"authorization\");tag=\"atproto-space-credential\"")
        ),
        "signature must cover exactly (\"authorization\" \"atproto-space-audience\"), in order"
    );
    assert_eq!(
        with(
            "signature-input",
            Some(&input.replace(";tag=\"atproto-space-credential\"", ""))
        ),
        "signature tag must be atproto-space-credential"
    );
    assert_eq!(
        with("signature-input", Some(&format!("{input};alg=\"rsa\""))),
        "signature algorithm must be ecdsa-p256-sha256"
    );
    assert_eq!(
        with(
            "signature-input",
            Some(&format!("{input};keyid=\"did:key:zOther\""))
        ),
        "signature keyid does not match the credential key"
    );
    assert_eq!(
        with("signature", Some("atproto-space-credential=:AAAA:")),
        "invalid HTTP message signature"
    );
    // Other members of the dictionaries are passed over.
    let mut both = headers.clone();
    both.insert(
        "signature-input".into(),
        format!("other=(\"a\");tag=\"x\", {input}"),
    );
    both.insert(
        "signature".into(),
        format!("other=:AA==:, {}", headers["signature"]),
    );
    verify_request(reader(&both), &id).unwrap();
    let k256 = Keypair::generate(Curve::K256).public().did_key();
    assert_eq!(
        verify_request(reader(&headers), &k256).unwrap_err().message,
        "signature key must be a P-256 did:key"
    );
}

fn token(kind: Kind, make: &Make, key: &Keypair) -> String {
    create(kind, make, key, NOW).unwrap()
}

#[test]
fn makes_and_checks_each_kind_of_token() {
    let member = Keypair::generate(Curve::K256);
    let ephemeral = Keypair::generate(Curve::P256).public().did_key();
    let aud = space_host_aud(AUTHORITY);
    assert_eq!(aud, "did:plc:authority#atproto_pds");

    let delegation = token(
        Kind::Delegation,
        &Make {
            iss: "did:plc:member",
            sub: SPACE,
            aud: Some(&aud),
            ..Make::default()
        },
        &member,
    );
    let read = verify(
        Kind::Delegation,
        &delegation,
        &member.public(),
        &Expect {
            aud: Some(&aud),
            sub: Some(SPACE),
        },
        NOW,
    )
    .unwrap();
    assert_eq!(
        (read.iss.as_str(), read.exp - read.iat, read.jti.len()),
        ("did:plc:member", 60, 16)
    );

    let authority = Keypair::generate(Curve::K256);
    let credential = token(
        Kind::Credential,
        &Make {
            iss: AUTHORITY,
            sub: SPACE,
            key_id: Some(&ephemeral),
            lifetime: Some(CREDENTIAL_MAX_AGE),
            ..Make::default()
        },
        &authority,
    );
    let read = verify(
        Kind::Credential,
        &credential,
        &authority.public(),
        &Expect::default(),
        NOW,
    )
    .unwrap();
    assert_eq!(read.cnf.as_deref(), Some(ephemeral.as_str()));
    assert_eq!(read.aud, None);

    let app = Keypair::generate(Curve::P256);
    let attestation = token(
        Kind::ClientAttestation,
        &Make {
            iss: "https://app.example/client.json",
            sub: "https://app.example/client.json",
            aud: Some(&aud),
            kid: Some("key-1"),
            ..Make::default()
        },
        &app,
    );
    let (header, _) = parse(Kind::ClientAttestation, &attestation).unwrap();
    assert_eq!(header["kid"], "key-1");
    verify(
        Kind::ClientAttestation,
        &attestation,
        &app.public(),
        &Expect::default(),
        NOW,
    )
    .unwrap();
}

#[test]
fn refuses_to_make_a_token_its_kind_does_not_allow() {
    let key = Keypair::generate(Curve::K256);
    let made = |kind, make: &Make| create(kind, make, &key, NOW).unwrap_err().message;
    assert_eq!(
        made(
            Kind::Delegation,
            &Make {
                iss: "a",
                sub: "b",
                ..Make::default()
            }
        ),
        "a Delegation token needs an aud"
    );
    assert_eq!(
        made(
            Kind::Credential,
            &Make {
                iss: "a",
                sub: "b",
                ..Make::default()
            }
        ),
        "a credential needs a key id"
    );
    assert_eq!(
        made(
            Kind::Delegation,
            &Make {
                iss: "a",
                sub: "b",
                aud: Some("c"),
                kid: Some("k"),
                ..Make::default()
            }
        ),
        "a Delegation token's kid is always #atproto"
    );
    let too_long = Make {
        iss: "a",
        sub: "b",
        key_id: Some("did:key:z"),
        lifetime: Some(CREDENTIAL_MAX_AGE + 1),
        ..Make::default()
    };
    assert_eq!(
        made(Kind::Credential, &too_long),
        "invalid space credential lifetime"
    );
}

/// A token whose header and payload are as given, signed by `key`.
fn forged(header: Value, payload: Value, key: &Keypair) -> String {
    let b = |v: &Value| URL_SAFE_NO_PAD.encode(v.to_string());
    let input = format!("{}.{}", b(&header), b(&payload));
    format!(
        "{input}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(input.as_bytes()))
    )
}

#[test]
fn refuses_tokens_that_do_not_hold() {
    let key = Keypair::generate(Curve::K256);
    let header = json!({"alg": "ES256K", "typ": "atproto-space-delegation+jwt", "kid": "#atproto"});
    let payload = json!({"iss": "did:plc:m", "sub": SPACE, "aud": "x", "iat": NOW, "exp": NOW + 60, "jti": "j"});
    let code = |kind, h: Value, p: Value| {
        let t = forged(h, p, &key);
        verify(kind, &t, &key.public(), &Expect::default(), NOW)
            .unwrap_err()
            .code
    };
    let set = |v: &Value, k: &str, x: Value| {
        let mut v = v.clone();
        v[k] = x;
        v
    };
    assert_eq!(parse(Kind::Delegation, "a.b").unwrap_err().code, "BadJwt");
    assert_eq!(
        parse(Kind::Delegation, "!.b.c").unwrap_err().message,
        "could not parse token header"
    );
    assert_eq!(
        code(Kind::Credential, header.clone(), payload.clone()),
        "BadJwtType"
    );
    assert_eq!(
        code(
            Kind::Delegation,
            set(&header, "alg", json!("")),
            payload.clone()
        ),
        "BadJwt"
    );
    assert_eq!(
        code(
            Kind::Delegation,
            set(&header, "kid", json!("#other")),
            payload.clone()
        ),
        "BadJwt"
    );
    assert_eq!(
        code(
            Kind::Delegation,
            header.clone(),
            set(&payload, "iss", json!(""))
        ),
        "BadJwtIss"
    );
    assert_eq!(
        code(
            Kind::Delegation,
            header.clone(),
            set(&payload, "exp", json!("soon"))
        ),
        "BadJwt"
    );
    assert_eq!(
        code(
            Kind::Delegation,
            header.clone(),
            set(&payload, "aud", Value::Null)
        ),
        "BadJwtAudience"
    );
    assert_eq!(
        code(
            Kind::Delegation,
            header.clone(),
            set(&payload, "jti", Value::Null)
        ),
        "BadJwt"
    );
    assert_eq!(
        code(
            Kind::Delegation,
            header.clone(),
            set(&payload, "exp", json!(NOW - 5))
        ),
        "JwtExpired"
    );

    let credential =
        json!({"alg": "ES256K", "typ": "atproto-space-credential+jwt", "kid": "#atproto"});
    let bound = set(&payload, "cnf", json!({"kid": "did:key:zDnaeexample"}));
    assert_eq!(
        code(Kind::Credential, credential.clone(), payload.clone()),
        "BadJwtCnf"
    );
    assert_eq!(
        code(
            Kind::Credential,
            credential.clone(),
            set(&bound, "exp", json!(NOW + CREDENTIAL_MAX_AGE + 1))
        ),
        "BadJwt"
    );
    assert_eq!(
        code(
            Kind::Credential,
            credential.clone(),
            set(
                &set(&bound, "iat", json!(NOW + 100)),
                "exp",
                json!(NOW + 200)
            )
        ),
        "BadJwt"
    );

    let attestation = json!({"alg": "ES256K", "typ": "atproto-client-attestation+jwt"});
    assert_eq!(
        code(
            Kind::ClientAttestation,
            attestation,
            set(&payload, "sub", json!("other"))
        ),
        "BadJwtIss"
    );

    let good = forged(header.clone(), payload.clone(), &key);
    let checked = |expect: &Expect, k: &Keypair| {
        verify(Kind::Delegation, &good, &k.public(), expect, NOW)
            .unwrap_err()
            .code
    };
    assert_eq!(
        checked(
            &Expect {
                aud: Some("y"),
                sub: None
            },
            &key
        ),
        "BadJwtAudience"
    );
    assert_eq!(
        checked(
            &Expect {
                aud: None,
                sub: Some("z")
            },
            &key
        ),
        "BadJwtSub"
    );
    assert_eq!(
        checked(&Expect::default(), &Keypair::generate(Curve::K256)),
        "BadJwtSignature"
    );
    assert_eq!(
        checked(&Expect::default(), &Keypair::generate(Curve::P256)),
        "BadJwtSignature"
    );
    assert_eq!(
        verify(
            Kind::Delegation,
            &good,
            &key.public(),
            &Expect::default(),
            NOW
        )
        .unwrap()
        .sub,
        SPACE
    );
    assert!(format!("{}", parse(Kind::Delegation, "a").unwrap_err()).starts_with("BadJwt: "));
}
