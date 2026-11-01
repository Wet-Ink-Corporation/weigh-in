//! Relating an item to others, or to the digest: supersede, link and acknowledge.

use anyhow::{Result, bail};
use weighin_core::event::{Acknowledged, LedgerEvent, Linked, Superseded};
use weighin_core::{Item, Status};

use super::Change;

pub(super) fn supersede_event(item: &Item, by: &str, reason: &str) -> Change {
    Change {
        event: LedgerEvent::ItemSuperseded(Superseded {
            item_id: item.id.clone(),
            by: by.to_owned(),
            reason: reason.to_owned(),
        }),
        receipt: format!("{} superseded by {by}", item.id),
    }
}

pub(super) fn link_event(item: &Item, kind: &str, target: &str, reason: Option<&str>) -> Change {
    Change {
        event: LedgerEvent::ItemLinked(Linked {
            item_id: item.id.clone(),
            kind: kind.to_owned(),
            target: target.to_owned(),
            reason: reason.unwrap_or("linked").to_owned(),
        }),
        receipt: format!("linked {} → {kind} {target}", item.id),
    }
}

pub(super) fn ack_event(item: &Item, reason: Option<&str>) -> Result<Change> {
    if item.status != Status::Defaulted {
        bail!(
            "{} is {}, only defaults are acknowledged",
            item.id,
            item.status.as_str()
        );
    }
    Ok(Change {
        event: LedgerEvent::ItemAcknowledged(Acknowledged {
            item_id: item.id.clone(),
            reason: reason
                .unwrap_or("left as defaulted after the digest")
                .to_owned(),
        }),
        receipt: format!("acknowledged default on {}", item.id),
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::ops::test_support::{ONE_WAY, after, grounded, id_of};

    #[test]
    #[cfg_attr(miri, ignore = "opens SQLite through FFI")]
    fn ack_refuses_an_item_that_was_not_defaulted() -> Result<()> {
        let out = after(&grounded(), &[serde_json::json!({"op": "ack", "id": "@1"})])?;

        assert_eq!(
            out,
            [format!(
                "✗ {} is grounded, only defaults are acknowledged",
                id_of(ONE_WAY)
            )]
        );
        Ok(())
    }
}
