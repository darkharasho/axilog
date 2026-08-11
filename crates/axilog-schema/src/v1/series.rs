use serde::Serialize;

/// One time series, in the format's single series envelope.
///
/// `enc` is `"raw"` (a plain array of values) or `"rle"` (an array of
/// `[value, run_length]` pairs), chosen per series by whichever serializes
/// smaller. `len` is the DECODED length in both cases, so a consumer can
/// allocate before decoding and validate after.
///
/// WvW per-second series are dominated by long zero runs -- a player idle
/// for 400 seconds is one pair rather than 400 characters. Base64 typed
/// arrays are deliberately NOT an option here: this format's calibration
/// workflow is diffing exports against GW2EI's, and opaque blobs destroy
/// that. A third `enc` value may be added later without breaking consumers
/// that already switch on the tag -- that is why the tag exists.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SeriesOut {
    pub interval_ms: u64,
    /// Decoded length, NOT `data.len()`.
    pub len: u64,
    pub enc: &'static str,
    pub data: Vec<serde_json::Value>,
}

impl SeriesOut {
    pub fn encode_u64(interval_ms: u64, values: &[u64]) -> Self {
        Self::encode(interval_ms, values, |v| serde_json::json!(v))
    }

    pub fn encode_f64(interval_ms: u64, values: &[f64]) -> Self {
        Self::encode(interval_ms, values, |v| serde_json::json!(v))
    }

    /// Shared encoder. `runs` are built structurally (equality on the
    /// serialized JSON value), so the same rule applies to integer and
    /// float series without duplicating the run detection.
    fn encode<T: Copy + PartialEq>(
        interval_ms: u64,
        values: &[T],
        to_json: fn(T) -> serde_json::Value,
    ) -> Self {
        let raw: Vec<serde_json::Value> = values.iter().copied().map(to_json).collect();

        let mut runs: Vec<serde_json::Value> = Vec::new();
        let mut i = 0usize;
        while i < values.len() {
            let mut j = i + 1;
            while j < values.len() && values[j] == values[i] {
                j += 1;
            }
            runs.push(serde_json::json!([to_json(values[i]), (j - i) as u64]));
            i = j;
        }

        // "Smaller" is measured on the actual serialized bytes -- the only
        // definition that matters for a wire format, and cheap at these
        // sizes. A tie goes to `raw`, which needs no decoder.
        let raw_len = serde_json::to_string(&raw).map(|s| s.len()).unwrap_or(usize::MAX);
        let rle_len = serde_json::to_string(&runs).map(|s| s.len()).unwrap_or(usize::MAX);
        if rle_len < raw_len {
            SeriesOut { interval_ms, len: values.len() as u64, enc: "rle", data: runs }
        } else {
            SeriesOut { interval_ms, len: values.len() as u64, enc: "raw", data: raw }
        }
    }

    /// Reference decoder for integer series. The SDKs port this; it is
    /// public so there is exactly one definition of the algorithm.
    pub fn decode_u64(&self) -> Vec<u64> {
        self.decode(|v| v.as_u64().unwrap_or_default())
    }

    /// Reference decoder for float series.
    pub fn data_f64(&self) -> Vec<f64> {
        self.decode(|v| v.as_f64().unwrap_or_default())
    }

    fn decode<T>(&self, from_json: fn(&serde_json::Value) -> T) -> Vec<T> {
        let mut out = Vec::with_capacity(self.len as usize);
        match self.enc {
            "rle" => {
                for pair in &self.data {
                    let run = pair[1].as_u64().unwrap_or_default();
                    for _ in 0..run {
                        out.push(from_json(&pair[0]));
                    }
                }
            }
            _ => {
                for v in &self.data {
                    out.push(from_json(v));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_arbitrary_values() {
        // Deterministic pseudo-random cases -- no `rand` dependency, and a
        // fixed sequence keeps a failure reproducible.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for len in [0usize, 1, 2, 3, 17, 256, 1843] {
            for sparsity in [1u64, 4, 64, 4096] {
                let values: Vec<u64> =
                    (0..len).map(|_| if next() % sparsity == 0 { next() % 10_000 } else { 0 }).collect();
                let s = SeriesOut::encode_u64(1000, &values);
                assert_eq!(s.decode_u64(), values, "round-trip failed for len={len} sparsity={sparsity}");
                assert_eq!(s.len as usize, values.len(), "len must be the DECODED length");
            }
        }
    }

    #[test]
    fn picks_the_smaller_of_the_two_encodings() {
        // A long zero run must choose RLE.
        let zeros = vec![0u64; 400];
        let s = SeriesOut::encode_u64(1000, &zeros);
        assert_eq!(s.enc, "rle", "400 zeros must encode as a run");
        assert_eq!(s.data.len(), 1, "400 zeros is ONE run pair");

        // Alternating values make RLE strictly worse (every run is length 1,
        // costing a nested array per element), so raw must win.
        let alternating: Vec<u64> = (0..64).map(|i| i as u64).collect();
        let s = SeriesOut::encode_u64(1000, &alternating);
        assert_eq!(s.enc, "raw", "run-free data must encode as raw");
    }

    #[test]
    fn an_empty_series_is_raw_and_empty() {
        let s = SeriesOut::encode_u64(1000, &[]);
        assert_eq!(s.len, 0);
        assert_eq!(s.enc, "raw");
        assert!(s.data.is_empty());
        assert_eq!(s.decode_u64(), Vec::<u64>::new());
    }

    #[test]
    fn serializes_to_the_documented_json_shape() {
        // Ten zeros then a value: raw is 23 bytes, RLE is 14, so RLE wins
        // and we can pin the documented pair shape. (A shorter run like
        // [0,0,0,5] is 9 bytes raw vs 13 as RLE -- raw correctly wins there,
        // which is the encoder working, not a bug.)
        let s = SeriesOut::encode_u64(1000, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5]);
        let v = serde_json::to_value(&s).expect("serializable");
        assert_eq!(v["interval_ms"], 1000);
        assert_eq!(v["len"], 11);
        assert_eq!(v["enc"], "rle");
        // RLE pairs are [value, run_length].
        assert_eq!(v["data"], serde_json::json!([[0, 10], [5, 1]]));
    }

    #[test]
    fn f64_values_round_trip_through_the_same_envelope() {
        let values = vec![0.0, 0.0, 1.5, 1.5, 1.5, 0.25];
        let s = SeriesOut::encode_f64(1000, &values);
        assert_eq!(s.len, 6);
        let decoded: Vec<f64> = s.data_f64();
        assert_eq!(decoded, values);
    }
}
