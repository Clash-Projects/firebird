/// Joins libmpv `af` pieces so equalizer and headphone correction occupy
/// one chain instead of replacing each other.
pub fn merge_equalizer_and_headphone(equalizer_af: &str, headphone_af: &str) -> String {
    let mut inner: Vec<&str> = Vec::new();

    for af in [equalizer_af, headphone_af] {
        let trimmed = af.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(body) = trimmed
            .strip_prefix("format=floatp,lavfi=[")
            .and_then(|s| s.strip_suffix(']'))
        {
            if !body.is_empty() {
                inner.push(body);
            }
            continue;
        }
        if let Some(body) = trimmed
            .strip_prefix("lavfi=[")
            .and_then(|s| s.strip_suffix(']'))
        {
            if !body.is_empty() {
                inner.push(body);
            }
            continue;
        }
        inner.push(trimmed);
    }

    if inner.is_empty() {
        return String::new();
    }
    format!("format=floatp,lavfi=[{}]", inner.join(","))
}

#[cfg(test)]
mod tests {
    use super::merge_equalizer_and_headphone;

    #[test]
    fn merges_empty_and_single_chains() {
        assert_eq!(merge_equalizer_and_headphone("", ""), "");
        assert_eq!(merge_equalizer_and_headphone("   ", "  "), "");
        assert_eq!(
            merge_equalizer_and_headphone("format=floatp,lavfi=[equalizer=f=31:g=2]", ""),
            "format=floatp,lavfi=[equalizer=f=31:g=2]"
        );
        assert_eq!(
            merge_equalizer_and_headphone("", "lavfi=[equalizer=f=1000:g=-1.5]"),
            "format=floatp,lavfi=[equalizer=f=1000:g=-1.5]"
        );
    }

    #[test]
    fn merges_both_chains_into_one_lavfi() {
        let eq = "format=floatp,lavfi=[equalizer=f=63:width_type=q:w=1.41:g=3]";
        let hp = "format=floatp,lavfi=[volume=-2.0dB,equalizer=f=200:t=q:w=1.2:g=-2.0]";
        assert_eq!(
            merge_equalizer_and_headphone(eq, hp),
            "format=floatp,lavfi=[equalizer=f=63:width_type=q:w=1.41:g=3,volume=-2.0dB,equalizer=f=200:t=q:w=1.2:g=-2.0]"
        );
    }

    #[test]
    fn absorbs_unwrapped_and_empty_lavfi_bodies() {
        assert_eq!(
            merge_equalizer_and_headphone("format=floatp,lavfi=[]", "lavfi=[]"),
            ""
        );
        assert_eq!(
            merge_equalizer_and_headphone("volume=-1dB", "lavfi=[aresample=48000]"),
            "format=floatp,lavfi=[volume=-1dB,aresample=48000]"
        );
    }
}
