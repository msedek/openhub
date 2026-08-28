//! `openlogi diag onboard` — read HID++ `0x8100` onboard-profile state.
//!
//! Read-only, deliberately. Onboard memory holds the configuration the user
//! saved on the device itself; a wrong write leaves it in a state only the
//! vendor's Windows software can repair. Writes come later, behind a flag,
//! once reads are proven.

use anyhow::{Context, Result};
use clap::Args;
use ghub_models::{DeviceModel, model_for_hidpp_id, model_for_usb_id};
use openlogi_hid::{DeviceMode, WriteError};

use crate::cmd::diag::select_device_with_product_ids;

/// HID++ feature id for OnboardProfiles, named once so the message below and
/// the device selection cannot drift apart.
const ONBOARD_PROFILES: u16 = 0x8100;

/// Where a wrong model-table entry is fixed, quoted in the mismatch warnings.
const CATALOG_PATH: &str = "crates/ghub-models/src/catalog.rs";

#[derive(Debug, Args)]
pub struct OnboardArgs {
    /// Run against the device whose name contains this string
    /// (case-insensitive) instead of auto-selecting. Useful when several
    /// devices are paired (e.g. a mouse and a keyboard over Bluetooth).
    #[arg(long, value_name = "NAME")]
    pub device: Option<String>,
}

pub async fn run(args: OnboardArgs) -> Result<()> {
    // 0x8100 OnboardProfiles — auto-skip devices (every productivity mouse and
    // keyboard) that do not expose it.
    let (route, name, product_ids) =
        select_device_with_product_ids(args.device.as_deref(), &[ONBOARD_PROFILES]).await?;
    println!("device: {name} ({route})");

    let state = match openlogi_hid::get_onboard_state(&route).await {
        Ok(state) => state,
        Err(WriteError::FeatureUnsupported { feature_hex }) if feature_hex == ONBOARD_PROFILES => {
            // The expected answer for every non-gaming Logitech device, so say
            // it in a sentence instead of returning an error the shell reports
            // as a failure.
            println!(
                "  this device does not expose HID++ 0x8100 (onboard profiles) — nothing to \
                 read. That is normal for anything but a G-series gaming device."
            );
            return Ok(());
        }
        Err(e) => return Err(e).context("read HID++ 0x8100 onboard-profile state"),
    };

    let info = state.info;
    println!("  memory model id:      {:#04x}", info.memory_model_id);
    println!("  profile format id:    {:#04x}", info.profile_format_id);
    println!("  macro format id:      {:#04x}", info.macro_format_id);
    println!("  profile count:        {}", info.profile_count);
    println!("  profile count (OOB):  {}", info.profile_count_oob);
    println!("  button count:         {}", info.button_count);
    println!("  sector count:         {}", info.sector_count);
    println!("  sector size:          {} bytes", info.sector_size);
    println!("  mechanical layout:    {:#04x}", info.mechanical_layout);
    println!("  various info:         {:#04x}", info.various_info);
    println!("  device mode:          {}", describe_mode(state.mode));
    println!("  active profile:       {}", state.current_profile);

    // A device that answers `0x8100` but reports no buttons has not really
    // answered: a short response arrives zero-padded, so a wrong feature or
    // function index reads as a mouse with no buttons rather than as an error.
    if info.button_count == 0 {
        println!(
            "  ! button count is 0, which no physical device reports. Treat this as a failed \
             read — wrong feature or function index, or the device did not answer — not as a \
             device without buttons."
        );
    }

    print_model_cross_check(&product_ids, info.button_count, info.profile_count);

    Ok(())
}

/// Spell out a device mode for a human, including the case where the device
/// answered with a value this build does not know.
fn describe_mode(mode: Option<DeviceMode>) -> &'static str {
    match mode {
        Some(DeviceMode::Onboard) => "Onboard (the device runs its own stored profile)",
        Some(DeviceMode::Host) => "Host (software drives the device)",
        None => "unknown — the device reported a mode value this build does not know",
    }
}

/// Find the first of `product_ids` the model table recognises.
///
/// A device reports several: its wireless WPID through the receiver and the
/// per-transport PIDs from HID++ `0x0003`. Either kind may be the one the
/// table was written against, so both lookups are tried for each.
fn resolve_model(product_ids: &[u16]) -> Option<&'static DeviceModel> {
    product_ids
        .iter()
        .find_map(|id| model_for_hidpp_id(*id).or_else(|| model_for_usb_id(*id)))
}

/// Compare what the device said against what the model table claims for it.
///
/// A disagreement means the table is wrong — the device is the authority — and
/// finding that here is much cheaper than finding it in the GUI.
fn print_model_cross_check(product_ids: &[u16], button_count: u8, profile_count: u8) {
    let ids = product_ids
        .iter()
        .map(|id| format!("{id:#06x}"))
        .collect::<Vec<_>>()
        .join(", ");
    println!("  reported product ids: [{ids}]");

    let Some(model) = resolve_model(product_ids) else {
        println!("  model table:          no entry matches any of those ids");
        return;
    };

    println!(
        "  model table:          {} ({})",
        model.display_name, model.id
    );

    let table_buttons = model.slots.len();
    if usize::from(button_count) == table_buttons {
        println!("  buttons:              {button_count} — matches the model table");
    } else {
        println!(
            "  ! buttons:            device reports {button_count}, model table lists \
             {table_buttons} — the device is right, {CATALOG_PATH} needs fixing"
        );
    }

    if profile_count == model.onboard_profile_count {
        println!("  profiles:             {profile_count} — matches the model table");
    } else {
        println!(
            "  ! profiles:           device reports {profile_count}, model table lists {} — \
             the device is right, {CATALOG_PATH} needs fixing",
            model.onboard_profile_count
        );
    }
}

#[cfg(test)]
mod resolve_model_tests {
    use super::resolve_model;

    #[test]
    fn resolves_the_g703_from_its_wireless_wpid() {
        let model = resolve_model(&[0x4086]).expect("the G703 is in the table");

        assert_eq!(model.id, "g703_hero");
    }

    #[test]
    fn resolves_the_g703_from_its_wired_product_id() {
        // Plugging the cable in changes which id the device reports; it must
        // still resolve to the same model, or the cross-check would silently
        // stop running.
        let model = resolve_model(&[0xc090]).expect("the G703 is in the table");

        assert_eq!(model.id, "g703_hero");
    }

    #[test]
    fn skips_ids_the_table_does_not_know() {
        // A device reports several ids and only one of them is the model's.
        let model = resolve_model(&[0xffff, 0x4086]).expect("the G703 is in the table");

        assert_eq!(model.id, "g703_hero");
    }

    #[test]
    fn an_entirely_unknown_device_resolves_to_nothing() {
        assert!(resolve_model(&[0xffff]).is_none());
        assert!(resolve_model(&[]).is_none());
    }
}
