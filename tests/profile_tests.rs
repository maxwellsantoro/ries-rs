//! Integration tests for the profile module

use ries_rs::profile::{Profile, UserConstant};
use ries_rs::symbol::{NumType, Symbol};

/// Test creating an empty profile
#[test]
fn test_empty_profile() {
    let profile = Profile::new();
    assert!(profile.constants.is_empty());
    assert!(profile.symbol_names.is_empty());
    assert!(profile.symbol_weights.is_empty());
    assert!(profile.includes.is_empty());
}

/// Test profile merging
#[test]
fn test_profile_merge() {
    let mut p1 = Profile::new();
    p1.constants.push(UserConstant {
        weight: 4,
        name: "a".to_string(),
        description: "First constant".to_string(),
        value: 1.0,
        num_type: NumType::Integer,
    });
    p1.symbol_names.insert(Symbol::Pi, "π".to_string());

    let mut p2 = Profile::new();
    p2.constants.push(UserConstant {
        weight: 5,
        name: "b".to_string(),
        description: "Second constant".to_string(),
        value: 2.0,
        num_type: NumType::Integer,
    });
    p2.symbol_names.insert(Symbol::E, "ℯ".to_string());

    let merged = p1.merge(p2);

    assert_eq!(merged.constants.len(), 2);
    assert_eq!(merged.symbol_names.len(), 2);
}

/// Test that later profiles override earlier ones
#[test]
fn test_profile_override() {
    let mut p1 = Profile::new();
    p1.constants.push(UserConstant {
        weight: 4,
        name: "a".to_string(),
        description: "Original".to_string(),
        value: 1.0,
        num_type: NumType::Integer,
    });

    let mut p2 = Profile::new();
    p2.constants.push(UserConstant {
        weight: 8,
        name: "a".to_string(),
        description: "Override".to_string(),
        value: 2.0,
        num_type: NumType::Integer,
    });

    let merged = p1.merge(p2);

    // Should have only one constant (the override)
    assert_eq!(merged.constants.len(), 1);
    assert_eq!(merged.constants[0].value, 2.0);
    assert_eq!(merged.constants[0].weight, 8);
}

/// Test UserConstant structure
#[test]
fn test_user_constant() {
    let constant = UserConstant {
        weight: 10,
        name: "gamma".to_string(),
        description: "Euler's constant".to_string(),
        value: 0.5772156649,
        num_type: NumType::Transcendental,
    };

    assert_eq!(constant.weight, 10);
    assert_eq!(constant.name, "gamma");
    assert!((constant.value - 0.5772156649).abs() < 1e-10);
}

/// Test symbol name customization
#[test]
fn test_symbol_names() {
    let mut profile = Profile::new();
    profile.symbol_names.insert(Symbol::Pi, "π".to_string());
    profile.symbol_names.insert(Symbol::E, "ℯ".to_string());
    profile.symbol_names.insert(Symbol::Phi, "φ".to_string());

    assert_eq!(profile.symbol_names.get(&Symbol::Pi), Some(&"π".to_string()));
    assert_eq!(profile.symbol_names.get(&Symbol::E), Some(&"ℯ".to_string()));
    assert_eq!(profile.symbol_names.get(&Symbol::Phi), Some(&"φ".to_string()));
}

/// Test symbol weight customization
#[test]
fn test_symbol_weights() {
    let mut profile = Profile::new();
    profile.symbol_weights.insert(Symbol::LambertW, 20);
    profile.symbol_weights.insert(Symbol::Pi, 15);

    assert_eq!(profile.symbol_weights.get(&Symbol::LambertW), Some(&20));
    assert_eq!(profile.symbol_weights.get(&Symbol::Pi), Some(&15));
}
