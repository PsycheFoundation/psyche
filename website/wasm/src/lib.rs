use psyche_coordinator::model::LLMArchitecture;
use psyche_core::LearningRateSchedule;
use psyche_solana_coordinator::{ClientId, CoordinatorAccount, coordinator_account_from_bytes};
use serde::ser::Serialize;
use ts_rs::TS;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_COORDINATOR_DEF: &str = r#"
import { CoordinatorInstanceState } from "./CoordinatorInstanceState.js";
import { ClientId } from "./ClientId.js";
import { LearningRateSchedule } from "./LearningRateSchedule.js";

export type PsycheCoordinator = CoordinatorInstanceState;
"#;

#[wasm_bindgen(unchecked_return_type = "PsycheCoordinator")]
pub fn load_coordinator_from_bytes(bytes: Vec<u8>) -> Result<JsValue, JsError> {
    Ok((coordinator_account_from_bytes(&bytes)?.state.serialize(
        &serde_wasm_bindgen::Serializer::new().serialize_large_number_types_as_bigints(true),
    ))?)
}

#[wasm_bindgen]
pub fn lr_at_step(
    #[wasm_bindgen(unchecked_param_type = "LearningRateSchedule")] lr: JsValue,
    step: u32,
) -> Result<f64, JsError> {
    let lr: LearningRateSchedule = serde_wasm_bindgen::from_value(lr)?;
    Ok(lr.get_lr(step))
}

#[allow(dead_code)]
#[derive(TS)]
#[ts(export)]
pub struct DummyCoordinatorAccount(CoordinatorAccount);

#[allow(dead_code)]
#[derive(TS)]
#[ts(export)]
pub struct DummyClientId(ClientId);

// Export types that are now in ModelExtraData but still needed by the website
#[allow(dead_code)]
#[derive(TS)]
#[ts(export)]
pub struct DummyLLMArchitecture(LLMArchitecture);

#[allow(dead_code)]
#[derive(TS)]
#[ts(export)]
pub struct DummyLearningRateSchedule(LearningRateSchedule);
