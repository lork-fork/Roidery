//! Tajima TAP: DST's stitch records with no header (see [`crate::dst`]).

use crate::{Design, FormatError, dst};

pub fn write(design: &Design) -> Result<Vec<u8>, FormatError> {
    dst::write_records(design)
}

pub fn read(bytes: &[u8]) -> Result<Design, FormatError> {
    dst::read_records(bytes, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{sample, sewn};

    #[test]
    fn round_trips_the_sample_as_dst_records() {
        let d = sample();
        let bytes = write(&d).unwrap();
        assert_eq!(bytes.len() % 3, 0);
        // The same records as a DST file's body.
        assert_eq!(&bytes[..], &dst::write(&d).unwrap()[512..]);
        let back = read(&bytes).unwrap();
        assert_eq!(sewn(&back), sewn(&d));
        // Read as DST reads them (a long run of jumps reads as a trim too).
        assert_eq!(back.stitches, dst::read(&dst::write(&d).unwrap()).unwrap().stitches);
        assert_eq!(back.color_changes(), 1);
    }
}
