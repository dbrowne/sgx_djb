// Copyright (c) 2022 The MobileCoin Foundation

//! Types used for sealing and unsealing of secrets

use core::{mem, result::Result as CoreResult};
use mc_sgx_core_types::FfiError;
use mc_sgx_tservice_sys_types::{sgx_aes_gcm_data_t, sgx_sealed_data_t};

pub type Result<T> = CoreResult<T, FfiError>;

/// AES GCM(Galois/Counter mode) Data
///
/// Wraps up a `&[u8]` since [`mc-sgx-tservice-sys-types::sgx_aes_gcm_data_t`]
/// is a dynamically sized type
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct AesGcmData<'a> {
    bytes: &'a [u8],
}

impl<'a> TryFrom<&'a [u8]> for AesGcmData<'a> {
    type Error = FfiError;
    fn try_from(bytes: &'a [u8]) -> Result<Self> {
        let payload_size = Self::payload_size(bytes)?;
        if bytes.len() < payload_size + mem::size_of::<sgx_aes_gcm_data_t>() {
            Err(FfiError::InvalidInputLength)
        } else {
            Ok(Self { bytes })
        }
    }
}

impl<'a> AesGcmData<'a> {
    /// The size of the payload (encrypted data + mac text)
    ///
    /// This represents the dynamic data at the end of the
    /// [`mc-sgx-tservice-sys-types::sgx_aes_gcm_data_t`].
    fn payload_size(bytes: &[u8]) -> Result<usize> {
        const SIZE: usize = mem::size_of::<u32>();

        let mut size_bytes: [u8; SIZE] = [0; SIZE];
        let bytes = bytes.get(..SIZE).ok_or(FfiError::InvalidInputLength)?;
        size_bytes.copy_from_slice(bytes);

        let size = u32::from_le_bytes(size_bytes);

        Ok(size as usize)
    }
}

/// Sealed data
///
/// An opaque wrapper around `&[u8]` which is meant to be interpreted as
/// [`mc-sgx-tservice-sys-types::sgx_sealed_data_t`].
/// The [`mc-sgx-tservice-sys-types::sgx_sealed_data_t`] is a dynamically sized
/// type.
/// There is no need to directly access any of the underlying types members.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SealedData<'a> {
    bytes: &'a [u8],
}

impl<'a> TryFrom<&'a [u8]> for SealedData<'a> {
    type Error = FfiError;
    fn try_from(bytes: &'a [u8]) -> Result<Self> {
        let offset = mem::size_of::<sgx_sealed_data_t>() - mem::size_of::<sgx_aes_gcm_data_t>();
        let aes_gcm_bytes = bytes.get(offset..).ok_or(FfiError::InvalidInputLength)?;
        AesGcmData::try_from(aes_gcm_bytes)?;
        Ok(Self { bytes })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use core::{mem, slice};
    use mc_sgx_tservice_sys_types::sgx_aes_gcm_data_t;
    use yare::parameterized;

    // The buffer size of the byte types.
    // Extra trailing bytes (256) to store the _payload_
    const BUFFER_SIZE: usize = mem::size_of::<sgx_sealed_data_t>() + 256;

    /// Converts sealed data to bytes.
    ///
    /// The returned bytes will be larger than the size of `sgx_sealed_data_t`
    /// in order to contain the `encrypted_data` and optional `mac_text`.
    /// The [`sgx_sealed_data_t.plain_text_offset`] and
    /// [`sgx_sealed_data_t.aes_data.payload_size`] will be updated to account
    /// for the provided `encrypted_data` and `mac_text`.
    ///
    /// #Arguments
    /// * `sealed_data` - The sealed data to start the buffer with.
    /// * `encrypted_data` - The encrypted part of the payload
    /// * `mac_text` - The MAC text of the payload
    #[allow(unsafe_code)]
    fn sealed_data_to_bytes(
        sealed_data: sgx_sealed_data_t,
        encrypted_data: &[u8],
        mac_text: Option<&[u8]>,
    ) -> [u8; BUFFER_SIZE] {
        let mut sealed_data = sealed_data;

        let mac_length = match mac_text {
            Some(text) => {
                sealed_data.plain_text_offset = encrypted_data.len() as u32;
                text.len() as u32
            }
            None => {
                sealed_data.plain_text_offset = 0;
                0
            }
        };
        sealed_data.aes_data.payload_size = encrypted_data.len() as u32 + mac_length;

        // SAFETY: This is a test only function. The size of `sealed_data` is
        // used for reinterpretation of `sealed_data` into a byte slice. The
        // slice is copied from prior to the leaving of this function ensuring
        // the raw pointer is not persisted.
        let alias_bytes: &[u8] = unsafe {
            slice::from_raw_parts(
                &sealed_data as *const sgx_sealed_data_t as *const u8,
                mem::size_of::<sgx_sealed_data_t>(),
            )
        };

        let mut bytes: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        bytes[..mem::size_of::<sgx_sealed_data_t>()].copy_from_slice(alias_bytes);

        let payload_offset = mem::size_of::<sgx_sealed_data_t>();
        let encrypted_data_end = payload_offset + encrypted_data.len();
        bytes[payload_offset..encrypted_data_end].copy_from_slice(encrypted_data);

        if let Some(text) = mac_text {
            let mac_offset = encrypted_data_end;
            let mac_end = mac_offset + text.len();
            bytes[mac_offset..mac_end].copy_from_slice(text);
        }
        bytes
    }

    /// Converts [`sgx_aes_gcm_data_t`] to bytes.
    ///
    /// The returned bytes will be larger than the size of
    /// [`sgx_aes_gcm_data_t`] in order to contain the `payload`.
    /// [`sgx_aes_gcm_data_t.payload_size`] will be updated to account for the
    /// provided `payload`.
    ///
    /// #Arguments
    /// * `aes_gcm_data` - The AES GCM d data to start the buffer with
    /// * `payload` - The payload to append to the `aes_gcm_data`
    #[allow(unsafe_code)]
    fn aes_gcm_data_to_bytes(
        aes_gcm_data: sgx_aes_gcm_data_t,
        payload: &[u8],
    ) -> [u8; BUFFER_SIZE] {
        let mut aes_gcm_data = aes_gcm_data;
        aes_gcm_data.payload_size = payload.len() as u32;

        // SAFETY: This is a test only function. The size of `sgx_aes_gcm_data_t`
        // is used for reinterpretation of `aes_gcm_data` into a byte slice. The
        // slice is copied from prior to the leaving of this function ensuring the
        // raw pointer is not persisted.
        let alias_bytes: &[u8] = unsafe {
            slice::from_raw_parts(
                &aes_gcm_data as *const sgx_aes_gcm_data_t as *const u8,
                mem::size_of::<sgx_aes_gcm_data_t>(),
            )
        };

        let mut bytes: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        bytes[..mem::size_of::<sgx_aes_gcm_data_t>()].copy_from_slice(alias_bytes);

        let payload_offset = mem::size_of::<sgx_aes_gcm_data_t>();
        let payload_end = payload_offset + payload.len();
        bytes[payload_offset..payload_end].copy_from_slice(payload);

        bytes
    }

    #[parameterized
    (
    short = {b"short"},
    long = {b"0123456789"},
    )
    ]
    fn aes_data_from_bytes(payload: &[u8]) {
        let bytes = aes_gcm_data_to_bytes(sgx_aes_gcm_data_t::default(), payload);
        assert_eq!(
            AesGcmData::payload_size(bytes.as_slice()).unwrap(),
            payload.len()
        );
    }

    #[test]
    fn buffer_just_big_enough_for_aes_gcm_data() {
        let bytes = aes_gcm_data_to_bytes(sgx_aes_gcm_data_t::default(), b"");
        let size = mem::size_of::<sgx_aes_gcm_data_t>();
        assert!(AesGcmData::try_from(&bytes[..size]).is_ok());
    }

    #[test]
    fn buffer_too_small_for_aes_gcm_data() {
        let bytes = aes_gcm_data_to_bytes(sgx_aes_gcm_data_t::default(), b"");
        let size = mem::size_of::<sgx_aes_gcm_data_t>() - 1;
        assert_eq!(
            AesGcmData::try_from(&bytes[..size]),
            Err(FfiError::InvalidInputLength)
        );
    }

    #[test]
    fn buffer_just_big_enough_for_payload() {
        let bytes = aes_gcm_data_to_bytes(sgx_aes_gcm_data_t::default(), b"1234");
        let size = mem::size_of::<sgx_aes_gcm_data_t>() + b"1234".len();
        assert!(AesGcmData::try_from(&bytes[..size]).is_ok());
    }

    #[test]
    fn buffer_too_small_for_payload() {
        let bytes = aes_gcm_data_to_bytes(sgx_aes_gcm_data_t::default(), b"1234");
        let size = (mem::size_of::<sgx_aes_gcm_data_t>() + b"1234".len()) - 1;
        assert_eq!(
            AesGcmData::try_from(&bytes[..size]),
            Err(FfiError::InvalidInputLength)
        );
    }

    #[parameterized
    (
    short = {b"short", Some(b"mac text")},
    long = {b"0123456789", Some(b"9876543210")},
    no_mac = {b"0123456789", None},
    )
    ]
    fn sealed_data_try_from_bytes(encrypted_data: &[u8], mac_text: Option<&[u8]>) {
        let bytes = sealed_data_to_bytes(sgx_sealed_data_t::default(), encrypted_data, mac_text);
        assert!(SealedData::try_from(bytes.as_slice()).is_ok());
    }

    #[test]
    fn buffer_just_big_enough_for_sealed_data() {
        let bytes = sealed_data_to_bytes(sgx_sealed_data_t::default(), b"", None);
        let size = mem::size_of::<sgx_sealed_data_t>();
        assert!(SealedData::try_from(&bytes[..size]).is_ok());
    }

    #[test]
    fn buffer_too_small_for_sealed_data() {
        let bytes = sealed_data_to_bytes(sgx_sealed_data_t::default(), b"", None);
        let size = mem::size_of::<sgx_sealed_data_t>() - 1;
        assert_eq!(
            SealedData::try_from(&bytes[..size]),
            Err(FfiError::InvalidInputLength)
        );
    }

    #[test]
    fn buffer_just_big_enough_for_sealed_payload() {
        let bytes = sealed_data_to_bytes(sgx_sealed_data_t::default(), b"12", Some(b"34"));
        let payload_size = b"12".len() + b"34".len();
        let size = mem::size_of::<sgx_sealed_data_t>() + payload_size;
        assert!(SealedData::try_from(&bytes[..size]).is_ok());
    }

    #[test]
    fn buffer_too_small_for_sealed_payload() {
        let bytes = sealed_data_to_bytes(sgx_sealed_data_t::default(), b"12", Some(b"34"));
        let payload_size = b"12".len() + b"34".len();
        let size = (mem::size_of::<sgx_sealed_data_t>() + payload_size) - 1;
        assert_eq!(
            SealedData::try_from(&bytes[..size]),
            Err(FfiError::InvalidInputLength)
        );
    }

    #[test]
    fn sealed_data_just_big_enough_to_pass_on_aes_gcm() {
        let bytes = sealed_data_to_bytes(sgx_sealed_data_t::default(), b"", None);
        let size = mem::size_of::<sgx_sealed_data_t>() - mem::size_of::<sgx_aes_gcm_data_t>();

        // This will still fail, as the AesGcmData::TryFrom will fail.
        assert_eq!(
            SealedData::try_from(&bytes[..size]),
            Err(FfiError::InvalidInputLength)
        );
    }

    #[test]
    fn sealed_data_to_small_for_aes_gcm() {
        let bytes = sealed_data_to_bytes(sgx_sealed_data_t::default(), b"", None);
        let size = (mem::size_of::<sgx_sealed_data_t>() - mem::size_of::<sgx_aes_gcm_data_t>()) - 1;
        assert_eq!(
            SealedData::try_from(&bytes[..size]),
            Err(FfiError::InvalidInputLength)
        );
    }
}
