//! Execution price helpers including open-interest price impact.

pub const FACTOR_DENOMINATOR: u128 = 1_000_000;

/// Result of an execution price query for UI preview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionPrice {
    /// Oracle index price before price impact.
    pub price_without_impact: i128,
    /// Fill price after applying OI-based price impact.
    pub price_with_impact: i128,
}

/// Compute absolute price impact from the current OI imbalance.
///
/// Impact applies only when the trade would worsen the dominant side's share
/// of open interest (e.g. increasing longs when long OI already exceeds short OI).
pub fn compute_price_impact_amount(
    index_price: u128,
    size_delta_usd: u128,
    long_oi: u128,
    short_oi: u128,
    is_long: bool,
    is_increase: bool,
    price_impact_factor: u128,
) -> u128 {
    if size_delta_usd == 0 || price_impact_factor == 0 || index_price == 0 {
        return 0;
    }

    let total_oi = long_oi.saturating_add(short_oi);
    if total_oi == 0 {
        return 0;
    }

    let (imbalance, dominant_long) = if long_oi >= short_oi {
        (long_oi - short_oi, true)
    } else {
        (short_oi - long_oi, false)
    };

    let worsens_imbalance = (is_long == dominant_long) == is_increase;
    if !worsens_imbalance || imbalance == 0 {
        return 0;
    }

    index_price
        .saturating_mul(imbalance)
        .saturating_mul(price_impact_factor)
        / total_oi
        / FACTOR_DENOMINATOR
}

/// Apply signed price impact to an index price for the given trade direction.
pub fn apply_price_impact(
    index_price: u128,
    impact_amount: u128,
    is_long: bool,
    is_increase: bool,
) -> u128 {
    if impact_amount == 0 || index_price == 0 {
        return index_price;
    }

    match (is_long, is_increase) {
        (true, true) | (false, false) => index_price.saturating_add(impact_amount),
        (true, false) | (false, true) => index_price.saturating_sub(impact_amount),
    }
}

/// Compute execution prices for a position trade preview.
pub fn get_execution_price(
    index_price: u128,
    size_delta_usd: u128,
    long_oi: u128,
    short_oi: u128,
    is_long: bool,
    is_increase: bool,
    price_impact_factor: u128,
) -> ExecutionPrice {
    let price_without_impact = index_price as i128;
    let impact_amount = compute_price_impact_amount(
        index_price,
        size_delta_usd,
        long_oi,
        short_oi,
        is_long,
        is_increase,
        price_impact_factor,
    );
    let price_with_impact =
        apply_price_impact(index_price, impact_amount, is_long, is_increase) as i128;

    ExecutionPrice {
        price_without_impact,
        price_with_impact,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balanced_oi_has_no_impact() {
        let result = get_execution_price(100, 1_000, 5_000, 5_000, true, true, 50_000);
        assert_eq!(result.price_without_impact, 100);
        assert_eq!(result.price_with_impact, 100);
    }

    #[test]
    fn test_long_increase_with_long_oi_imbalance_increases_price() {
        // long_oi=8000, short_oi=2000 → imbalance=6000, total=10000
        // impact = 100 * 6000 * 100_000 / 10000 / 1_000_000 = 6
        let result = get_execution_price(100, 1_000, 8_000, 2_000, true, true, 100_000);
        assert_eq!(result.price_without_impact, 100);
        assert_eq!(result.price_with_impact, 106);
    }

    #[test]
    fn test_long_decrease_with_long_oi_imbalance_has_no_adverse_impact() {
        let result = get_execution_price(100, 1_000, 8_000, 2_000, true, false, 100_000);
        assert_eq!(result.price_with_impact, 100);
    }

    #[test]
    fn test_short_increase_with_short_oi_imbalance_worsens_execution_price() {
        let result = get_execution_price(100, 1_000, 2_000, 8_000, false, true, 100_000);
        assert_eq!(result.price_with_impact, 94);
    }
}
