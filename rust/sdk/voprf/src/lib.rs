#![cfg_attr(not(test), no_std)]

//! A VOPRF based on 2HashDH and a Chaum-Pedersen DLEQ proof.
//!
//! See the JKK14 paper for 2HashDH:
//!
//! > Jarecki, S., Kiayias, A., and H. Krawczyk, "Round-Optimal
//! > Password-Protected Secret Sharing and T-PAKE in the Password-Only Model",
//! > Lecture Notes in Computer Science pp. 233-253, DOI
//! > 10.1007/978-3-662-45608-8_13, 2014,
//! > <https://doi.org/10.1007/978-3-662-45608-8_13>.
//!
//! # Historical Note
//!
//! We considered using the IRTF draft [Oblivious Pseudorandom Functions
//! (OPRFs) using Prime-Order
//! Groups](https://datatracker.ietf.org/doc/draft-irtf-cfrg-voprf/). However,
//! their proof definition is optimized for batches, at the expense of
//! single-VOPRF performance. It requires the server to do 4 scalar-point
//! multiplications (for a small batch) instead of the 2 required by a
//! Chaum-Pedersen proof.

use core::fmt;
use curve25519_dalek::ristretto::{
    CompressedRistretto as CompressedPoint, RistrettoPoint as Point,
};
use curve25519_dalek::Scalar;
use digest::Digest;
use juicebox_sdk_marshalling::bytes;
use rand_core::CryptoRngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use subtle::ConstantTimeEq;
use zeroize::ZeroizeOnDrop;

mod dleq;

pub use dleq::Proof;

/// A Ristretto [`Point`] in both uncompressed and compressed forms.
///
/// Decompressing or compressing a point takes about 3 microseconds on a 2012
/// Intel laptop. Careful use of this struct helps avoid unnecessarily
/// decompressing and compressing points.
///
/// Note: Points are always serialized to bytes in compressed form only.
#[derive(Clone, Eq, ZeroizeOnDrop)]
struct DecompressedPoint {
    uncompressed: Point,
    compressed: CompressedPoint,
}

impl PartialEq for DecompressedPoint {
    fn eq(&self, other: &Self) -> bool {
        bool::from(self.compressed.ct_eq(&other.compressed))
    }
}

impl From<Point> for DecompressedPoint {
    fn from(uncompressed: Point) -> Self {
        Self {
            compressed: uncompressed.compress(),
            uncompressed,
        }
    }
}

impl TryFrom<CompressedPoint> for DecompressedPoint {
    type Error = &'static str;

    fn try_from(compressed: CompressedPoint) -> Result<Self, Self::Error> {
        match compressed.decompress() {
            Some(uncompressed) => Ok(Self {
                uncompressed,
                compressed,
            }),
            None => Err("decompression failed: not canonical point encoding"),
        }
    }
}

impl Serialize for DecompressedPoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        <CompressedPoint as bytes::Bytes>::serialize(&self.compressed, serializer)
    }
}

impl<'de> Deserialize<'de> for DecompressedPoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        <CompressedPoint as bytes::Bytes>::deserialize(deserializer)
            .and_then(|compressed| Self::try_from(compressed).map_err(serde::de::Error::custom))
    }
}

/// A hash of the secret OPRF input data.
#[derive(ZeroizeOnDrop)]
pub struct InputHash([u8; 64]);

impl fmt::Debug for InputHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InputHash(REDACTED)")
    }
}

impl InputHash {
    pub fn hash(input: &[u8]) -> Self {
        Self(Sha512::digest(input).into())
    }
}

/// What the server runs its computation over.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlindedInput(DecompressedPoint);

impl fmt::Debug for BlindedInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BlindedInput(REDACTED)")
    }
}

/// The server's result.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlindedOutput(DecompressedPoint);

impl fmt::Debug for BlindedOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BlindedOutput(REDACTED)")
    }
}

impl BlindedOutput {
    /// Low-level interface exposed for JKKX17 usage.
    pub fn to_point(self) -> Point {
        self.0.uncompressed
    }
}

impl From<Point> for BlindedOutput {
    /// Low-level interface exposed for JKKX17 usage.
    fn from(point: Point) -> Self {
        Self(DecompressedPoint::from(point))
    }
}

/// The overall VOPRF result.
///
/// This is computed from a cryptographic hash function, so the bytes should be
/// indistinguishable from random.
#[must_use]
#[derive(ZeroizeOnDrop)]
pub struct Output([u8; 64]);

impl fmt::Debug for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Output(REDACTED)")
    }
}

impl Output {
    pub fn expose_secret(&self) -> &[u8; 64] {
        &self.0
    }
}

/// The key used by the server to compute its result.
#[derive(Clone, Deserialize, Eq, Serialize, ZeroizeOnDrop)]
pub struct PrivateKey(#[serde(with = "bytes")] Scalar);

impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        bool::from(self.0.ct_eq(&other.0))
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateKey(REDACTED)")
    }
}

impl PrivateKey {
    /// Generates a new random private key.
    pub fn random(rng: &mut impl CryptoRngCore) -> Self {
        Self(Scalar::random(rng))
    }

    /// Low-level interface exposed for JKKX17 usage.
    pub fn as_scalar(&self) -> &Scalar {
        &self.0
    }
}

impl From<Scalar> for PrivateKey {
    /// Low-level interface exposed for JKKX17 usage.
    fn from(scalar: Scalar) -> Self {
        Self(scalar)
    }
}

/// The public key used to create and verify VOPRF proofs. It corresponds to a
/// [`PrivateKey`], which is used to evaluate the OPRF.
//
// This is represented in compressed form only:
// - The server only needs the compressed form.
// - The client needs to decompress the public key only to verify the proof,
//   which is done once and is already a fallible operation.
#[derive(Clone, Eq, Deserialize, PartialEq, Serialize)]
pub struct PublicKey(#[serde(with = "bytes")] CompressedPoint);

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PublicKey(")?;
        for byte in self.as_bytes() {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

impl PublicKey {
    /// Generates a public from the private key, using a somewhat expensive
    /// computation.
    pub fn new_from_private(private_key: &PrivateKey) -> Self {
        Self(Point::mul_base(&private_key.0).compress())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// Evaluates a VOPRF locally, directly using the private key and the input.
///
/// This gives the same result as a full client-server VOPRF interaction, but
/// it is much cheaper computationally.
pub fn unoblivious_evaluate(private_key: &PrivateKey, input: &[u8]) -> Output {
    let input_hash: [u8; 64] = Sha512::digest(input).into();
    let input_point = Point::from_uniform_bytes(&input_hash);
    let result = private_key.0 * input_point;
    hash_to_output(input, &result)
}

fn hash_to_output(input: &[u8], result: &Point) -> Output {
    Output(
        Sha512::new()
            .chain_update("Juicebox_VOPRF_2023_1;")
            // JKK14 includes the public key in the hash. This does not do so,
            // because there is no obvious single public key in JKKX17.
            .chain_update(to_be8(input.len()))
            .chain_update(input)
            .chain_update(result.compress().as_bytes())
            .finalize()
            .into(),
    )
}

/// Converts the provided integer into a 8 byte array in big-endian
/// (network) byte order or panics if it is too large to fit.
//
// TODO: Move the `to_be[N]` functions to a crate this can depend on, and
// update them all to not include `len` on panics.
fn to_be8(len: impl TryInto<u64>) -> [u8; 8] {
    // Note: `len` may be sensitive, so don't include it in the error message.
    match len.try_into() {
        Ok(len) => len.to_be_bytes(),
        Err(_) => panic!("integer larger than 8 bytes"),
    }
}

/// A random values produced by [`start`] that is needed to complete the VOPRF
/// on the client.
#[derive(ZeroizeOnDrop)]
pub struct BlindingFactor(Scalar);

impl fmt::Debug for BlindingFactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BlindingFactor(REDACTED)")
    }
}

/// Starts the VOPRF protocol on the client.
///
/// The client should send the returned [`BlindedInput`] to the server and
/// should keep the returned [`BlindingFactor`] secret. The blinding factor
/// must be provided to [`finalize`] later to complete the VOPRF.
pub fn start(input: &[u8], rng: &mut impl CryptoRngCore) -> (BlindingFactor, BlindedInput) {
    let input_point = Point::hash_from_bytes::<Sha512>(input);
    let blinding_factor = Scalar::random(rng);
    let blinded_input = BlindedInput(DecompressedPoint::from(input_point * blinding_factor));
    (BlindingFactor(blinding_factor), blinded_input)
}

/// Completes the VOPRF protocol on the client.
///
/// The `input` should be the same as given to `start`, and the
/// `blinding_factor` should be as returned from [`start`]. The
/// `blinded_output` should come from the server.
///
/// # Warning
///
/// The caller should call [`verify_proof`] before using the output.
pub fn finalize(
    input: &[u8],
    blinding_factor: &BlindingFactor,
    blinded_output: &BlindedOutput,
) -> Output {
    let result = blinded_output.0.uncompressed * Scalar::invert(&blinding_factor.0);
    hash_to_output(input, &result)
}

/// The client should call this to ensure that the server did the correct
/// computation.
///
/// The [`BlindedInput`] should be the result of [`start`].
/// The [`BlindedOutput`] and [`Proof`] should come from the server.
///
/// Note: This can only ensure the public key is consistent with the proof. The
/// caller must somehow ensure the public key is acceptable.
pub fn verify_proof(
    blinded_input: &BlindedInput,
    blinded_output: &BlindedOutput,
    public_key: &PublicKey,
    proof: &Proof,
) -> Result<(), &'static str> {
    let public_key = DecompressedPoint::try_from(public_key.0).map_err(|_| "invalid public key")?;
    dleq::verify_proof(&blinded_input.0, &public_key, &blinded_output.0, proof)
}

/// Runs the VOPRF evaluation on the server.
pub fn blind_evaluate(
    private_key: &PrivateKey,
    public_key: &PublicKey,
    blinded_input: &BlindedInput,
    rng: &mut impl CryptoRngCore,
) -> (BlindedOutput, Proof) {
    let blinded_output = DecompressedPoint::from(private_key.0 * blinded_input.0.uncompressed);
    let proof = dleq::generate_proof(
        rng,
        &private_key.0,
        &blinded_input.0,
        &public_key.0,
        &blinded_output,
    );
    (BlindedOutput(blinded_output), proof)
}

#[cfg(test)]
mod tests {
    use super::*;

    use core::num::NonZeroU32;
    use rand_core::{OsRng, RngCore};
    use serde::{de::DeserializeOwned, Deserialize, Serialize};
    use std::collections::VecDeque;
    use std::io::Write;
    use std::path::Path;

    #[test]
    fn test_basic() {
        for _ in 0..10 {
            let mut input = [0u8; 8];
            OsRng.fill_bytes(&mut input);
            let private_key = PrivateKey::random(&mut OsRng);
            let public_key = PublicKey::new_from_private(&private_key);
            let expected = unoblivious_evaluate(&private_key, &input);

            for _ in 0..3 {
                // unoblivious
                assert_eq!(expected.0, unoblivious_evaluate(&private_key, &input).0);

                // oblivious
                let (blinding_factor, blinded_input) = start(&input, &mut OsRng);
                let (blinded_output, proof) =
                    blind_evaluate(&private_key, &public_key, &blinded_input, &mut OsRng);
                assert!(verify_proof(&blinded_input, &blinded_output, &public_key, &proof).is_ok());
                assert_eq!(
                    expected.0,
                    finalize(&input, &blinding_factor, &blinded_output).0
                );
            }
        }
    }

    struct ManualRng {
        entropy: VecDeque<u8>,
    }

    impl rand_core::CryptoRng for ManualRng {}

    impl rand_core::RngCore for ManualRng {
        fn next_u32(&mut self) -> u32 {
            rand_core::impls::next_u32_via_fill(self)
        }

        fn next_u64(&mut self) -> u64 {
            rand_core::impls::next_u64_via_fill(self)
        }

        fn fill_bytes(&mut self, dest: &mut [u8]) {
            self.try_fill_bytes(dest).unwrap()
        }

        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
            if self.entropy.len() >= dest.len() {
                let drained = self.entropy.drain(0..dest.len());
                for (byte, pointer) in drained.zip(dest) {
                    *pointer = byte;
                }
                Ok(())
            } else {
                Err(rand_core::Error::from(
                    NonZeroU32::new(rand_core::Error::CUSTOM_START).unwrap(),
                ))
            }
        }
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct TestVector {
        name: String,
        inputs: TestInputs,
        outputs: TestOutputs,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct TestInputs {
        input: String,
        private_key_seed: String,
        blinding_factor_seed: String,
        beta_t_seed: String,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct TestOutputs {
        private_key: String,
        public_key: String,
        blinding_factor: String,
        blinded_input: String,
        blinded_output: String,
        proof_c: String,
        proof_beta_z: String,
        output: String,
    }

    fn run_with_inputs(inputs: &TestInputs) -> TestOutputs {
        let mut rng = ManualRng {
            entropy: [
                hex::decode(&inputs.private_key_seed).unwrap(),
                hex::decode(&inputs.blinding_factor_seed).unwrap(),
                hex::decode(&inputs.beta_t_seed).unwrap(),
            ]
            .into_iter()
            .flatten()
            .collect(),
        };
        let private_key = PrivateKey::random(&mut rng);
        let public_key = PublicKey::new_from_private(&private_key);

        let input = hex::decode(&inputs.input).unwrap();
        let (blinding_factor, blinded_input) = start(&input, &mut rng);
        let (blinded_output, proof) =
            blind_evaluate(&private_key, &public_key, &blinded_input, &mut rng);
        assert_eq!(rng.entropy.len(), 0);
        assert!(verify_proof(&blinded_input, &blinded_output, &public_key, &proof).is_ok());
        let output = finalize(&input, &blinding_factor, &blinded_output);

        assert_eq!(output.0, unoblivious_evaluate(&private_key, &input).0);

        TestOutputs {
            private_key: hex::encode(private_key.0.as_bytes()),
            public_key: hex::encode(public_key.0.as_bytes()),
            blinding_factor: hex::encode(blinding_factor.0.as_bytes()),
            blinded_input: hex::encode(blinded_input.0.compressed.as_bytes()),
            blinded_output: hex::encode(blinded_output.0.compressed.as_bytes()),
            proof_c: hex::encode(proof.c.as_bytes()),
            proof_beta_z: hex::encode(proof.beta_z.as_bytes()),
            output: hex::encode(output.0),
        }
    }

    fn test_vectors_from_file(path: &Path) {
        let file =
            std::fs::File::open(path).unwrap_or_else(|e| panic!("failed to open {path:?}: {e}"));
        let vectors: Vec<TestVector> = serde_json::from_reader(file).unwrap();
        for TestVector {
            name,
            inputs,
            outputs,
        } in &vectors
        {
            assert_eq!(
                outputs,
                &run_with_inputs(inputs),
                "{path}: {name}",
                path = path.display()
            );
        }
    }

    #[test]
    fn test_random_vectors() {
        test_vectors_from_file(Path::new("src/test_vectors.json"));
    }

    fn generate_test_inputs() -> TestInputs {
        let mut input = Vec::new();
        input.resize(OsRng.next_u32() as usize & 0x0f, 0);
        OsRng.fill_bytes(&mut input);

        let mut private_key_seed = [0u8; 64];
        OsRng.fill_bytes(&mut private_key_seed);

        let mut blinding_factor_seed = [0u8; 64];
        OsRng.fill_bytes(&mut blinding_factor_seed);

        let mut beta_t_seed = [0u8; 64];
        OsRng.fill_bytes(&mut beta_t_seed);

        TestInputs {
            input: hex::encode(input),
            private_key_seed: hex::encode(private_key_seed),
            blinding_factor_seed: hex::encode(blinding_factor_seed),
            beta_t_seed: hex::encode(beta_t_seed),
        }
    }

    #[allow(unused)]
    fn generate_and_save_random_vectors() {
        let vectors: Vec<TestVector> = (1..=10)
            .map(|i| {
                let inputs = generate_test_inputs();
                let outputs = run_with_inputs(&inputs);
                TestVector {
                    name: format!("random-test-{i:02}"),
                    inputs,
                    outputs,
                }
            })
            .collect();
        let mut file = std::fs::File::create("src/test_vectors.json").unwrap();
        serde_json::to_writer_pretty(&mut file, &vectors).unwrap();
        writeln!(file);
    }

    fn serialize_rt<S: DeserializeOwned + Serialize>(v: &S) -> (usize, S) {
        let serialized = juicebox_sdk_marshalling::to_vec(v).unwrap();
        let deserialized = juicebox_sdk_marshalling::from_slice::<S>(&serialized).unwrap();
        (serialized.len(), deserialized)
    }

    #[test]
    fn test_blinded_input_serialize() {
        let blinded_input = BlindedInput(DecompressedPoint::from(Point::random(&mut OsRng)));
        let (serialized_len, blinded_input2) = serialize_rt(&blinded_input);
        assert_eq!(34, serialized_len);
        assert_eq!(blinded_input.0.compressed, blinded_input2.0.compressed);
        assert_eq!(blinded_input.0.uncompressed, blinded_input2.0.uncompressed);
    }

    #[test]
    fn test_blinded_output_serialize() {
        let blinded_output = BlindedOutput(DecompressedPoint::from(Point::random(&mut OsRng)));
        let (serialized_len, blinded_output2) = serialize_rt(&blinded_output);
        assert_eq!(34, serialized_len);
        assert_eq!(blinded_output.0.compressed, blinded_output2.0.compressed);
        assert_eq!(
            blinded_output.0.uncompressed,
            blinded_output2.0.uncompressed
        );
    }

    #[test]
    fn test_private_key_serialize() {
        let private_key = PrivateKey::random(&mut OsRng);
        let (serialized_len, private_key2) = serialize_rt(&private_key);
        assert_eq!(34, serialized_len);
        assert_eq!(private_key.0, private_key2.0);
    }

    #[test]
    fn test_public_key_serialize() {
        let private_key = PrivateKey::random(&mut OsRng);
        let public_key = PublicKey::new_from_private(&private_key);
        let (serialized_len, public_key2) = serialize_rt(&public_key);
        assert_eq!(34, serialized_len);
        assert_eq!(public_key.0, public_key2.0);
    }

    #[test]
    fn test_public_key_debug() {
        let public_key = PublicKey(
            CompressedPoint::from_slice(
                &hex::decode("5c4bf4acff9c745d2c59c5ed4eb86b607d838b7dcc6a9399484a80ca83cf2634")
                    .unwrap(),
            )
            .unwrap(),
        );
        assert_eq!(
            format!("{public_key:?}"),
            "PublicKey(5c4bf4acff9c745d2c59c5ed4eb86b607d838b7dcc6a9399484a80ca83cf2634)"
        );
    }
}
