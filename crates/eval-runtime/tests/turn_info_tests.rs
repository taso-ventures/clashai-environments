use eval_runtime::TurnInfo;

#[test]
fn turn_info_round_trips_through_json() {
    let turn_info = TurnInfo {
        turn_number: 7,
        phase: "guess_phase".to_string(),
        active_players: vec!["1".to_string(), "3".to_string()],
        is_terminal: false,
        decision_kind: Some("reactive".to_string()),
        state_revision: Some("turn:7:phase:guess_phase".to_string()),
        step_deadline_ms: Some(1_500),
    };

    let json = serde_json::to_string(&turn_info).expect("serialize");
    let round_trip: TurnInfo = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(round_trip, turn_info);
    assert!(json.contains("\"decision_kind\":\"reactive\""));
    assert!(json.contains("\"step_deadline_ms\":1500"));
}

#[test]
fn turn_info_omits_none_optional_fields() {
    let turn_info = TurnInfo {
        turn_number: 0,
        phase: "lobby".to_string(),
        active_players: vec![],
        is_terminal: false,
        decision_kind: None,
        state_revision: None,
        step_deadline_ms: None,
    };

    let json = serde_json::to_string(&turn_info).expect("serialize");
    assert!(!json.contains("decision_kind"));
    assert!(!json.contains("state_revision"));
    assert!(!json.contains("step_deadline_ms"));

    let round_trip: TurnInfo = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(round_trip, turn_info);
}

#[test]
fn turn_info_deserializes_with_unknown_fields() {
    let json = r#"{
        "turn_number": 3,
        "phase": "action",
        "active_players": ["1"],
        "is_terminal": false,
        "future_field": "should_be_ignored"
    }"#;

    let turn_info: TurnInfo = serde_json::from_str(json).expect("forward compat");
    assert_eq!(turn_info.turn_number, 3);
    assert_eq!(turn_info.decision_kind, None);
}
