//! Engineering units and raw value conversion related implementation
use anyhow::Result;
use ctapi_sys::*;
use std::io::Error;

/// Convert engineering scale variable to raw I/O device scale
///
/// This is not necessary for tag functions as Citect SCADA performs scale conversion.
/// No scale conversion is needed for digital, string, or cases where there is no scale conversion
/// between I/O device and engineering values.
/// You need to know the scale specified for each variable in the Citect SCADA variable tag table.
///
/// # Examples
/// ```no_run
/// use ctapi_rs::*;
/// use ctapi_sys::*;
/// use ctapi_rs::constants::*;
/// let scale = CtScale::new(CtHScale::new(0.0, 32000.0), CtHScale::new(0.0, 100.0));
/// let result = ct_eng_to_raw(42.23, &scale, CT_SCALE_RANGE_CHECK);
/// println!("{result:?}");
/// assert!(result.is_ok());
/// ```
pub fn ct_eng_to_raw(value: f64, scale: &CtScale, mode: u32) -> Result<f64> {
    let mut result = 0.0;
    unsafe {
        if !ctEngToRaw(&mut result, value, scale, mode) {
            return Err(Error::last_os_error().into());
        }
    }
    Ok(result)
}

/// Convert raw I/O device scale variable to engineering scale
///
/// This is not necessary for tag functions as Citect SCADA performs scale conversion.
/// No scale conversion is needed for digital, string, or cases where there is no scale conversion
/// between I/O device and engineering values.
/// You need to know the scale specified for each variable in the Citect SCADA variable tag table.
///
/// # Examples
/// ```no_run
/// use ctapi_rs::*;
/// use ctapi_sys::*;
/// use ctapi_rs::constants::*;
/// let scale = CtScale::new(CtHScale::new(0.0, 32000.0), CtHScale::new(0.0, 100.0));
/// let result = ct_raw_to_eng(2000.0, &scale, CT_SCALE_RANGE_CHECK);
/// println!("{result:?}");
/// assert!(result.is_ok());
/// ```
pub fn ct_raw_to_eng(value: f64, scale: &CtScale, mode: u32) -> Result<f64> {
    let mut result = 0.0;
    unsafe {
        if !ctRawToEng(&mut result, value, scale, mode) {
            return Err(Error::last_os_error().into());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::CT_SCALE_RANGE_CHECK;

    #[test]
    fn test_eng_to_raw_conversion() {
        // Assume we have a scale: raw value 0-32000 corresponds to engineering value 0-100
        let scale = CtScale::new(
            CtHScale::new(0.0, 32000.0), // Raw scale
            CtHScale::new(0.0, 100.0),   // Engineering scale
        );

        // Test engineering value 50.0 should convert to raw value approximately 16000
        let result = ct_eng_to_raw(50.0, &scale, CT_SCALE_RANGE_CHECK);
        assert!(result.is_ok());
        let raw_value = result.unwrap();
        assert!((raw_value - 16000.0).abs() < 1.0); // Allow small floating point error
    }

    #[test]
    fn test_raw_to_eng_conversion() {
        let scale = CtScale::new(
            CtHScale::new(0.0, 32000.0), // Raw scale
            CtHScale::new(0.0, 100.0),   // Engineering scale
        );

        // Test raw value 16000 should convert to engineering value approximately 50.0
        let result = ct_raw_to_eng(16000.0, &scale, CT_SCALE_RANGE_CHECK);
        assert!(result.is_ok());
        let eng_value = result.unwrap();
        assert!((eng_value - 50.0).abs() < 0.1); // Allow small floating point error
    }
}
