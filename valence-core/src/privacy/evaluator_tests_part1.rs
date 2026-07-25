    use crate::actor::Actor;
    use crate::evaluator::{DatabaseEvaluator, DEFAULT_IN_MEMORY};
    
    use crate::schema::SchemaMetadata;
    use crate::schema_api::{
        Schema, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
        SchemaPrivacy,
    };
    use crate::privacy_policies::{common, owner};

    fn policies_with_read_allow(rules: &[&str]) -> SchemaPolicies {
        SchemaPolicies {
            read: Some(SchemaPolicyRules {
                allow: rules
                    .iter()
                    .map(|r| SchemaPolicyRule {
                        name: r.to_string(),
                        description: None,
                        evaluator: evaluator_for_rule_name(r),
                    })
                    .collect(),
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }
    }

    fn evaluator_for_rule_name(name: &str) -> Option<&'static dyn PolicyEvaluator> {
        let evaluator: PrivacyRule = match name {
            "PUBLIC_READ" => common::PUBLIC_READ,
            "AUTHENTICATED" => common::AUTHENTICATED,
            "SYSTEM_ONLY" => common::SYSTEM_ONLY,
            "BLOCK_ALL" => common::BLOCK_ALL,
            "OWNER_BY_ID" => owner::OWNER_BY_ID,
            "OWNER_BY_USER_FIELD" => owner::OWNER_BY_USER_FIELD,
            _ => return None,
        };
        Some(Box::leak(Box::new(evaluator)) as &'static dyn PolicyEvaluator)
    }

    fn schema_with_fields(fields: Vec<SchemaField>) -> SchemaMetadata {
        let schema = Box::leak(Box::new(Schema {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "public".to_string(),
                write: "user".to_string(),
            },
            policies: None,
            fields,
            edges: Vec::new(),
            connections: Vec::new(),
            side_effects: Vec::new(),
            iters: Vec::new(),
            composite_key: Vec::new(),
            traits: Vec::new(),
            ttl: None,
            ownership: None,
            meta: SchemaMeta {
                retention: "365 days".to_string(),
                row_count: 0,
                owner: "system".to_string(),
                description: None,
            },
        }));

        SchemaMetadata::from_schema(schema)
    }

    #[test]
    fn test_privacy_policy_evaluation_always_block() {
        use serde_json::json;
        let policy = PrivacyPolicy {
            always_allow: vec![],
            allow: vec![],
            block: vec![],
            always_block: vec![PrivacyRule {
                name: "block_all",
                description: None,
                check: |_record, _viewer| true,
            }],
        };

        let actor = Actor::Anonymous;
        let record = json!({});
        let result = PrivacyEvaluator::evaluate(&policy, &record, &actor);
        assert!(result.is_err());
    }

    #[test]
    fn test_privacy_policy_evaluation_allow() {
        use serde_json::json;
        let policy = PrivacyPolicy {
            always_allow: vec![],
            allow: vec![PrivacyRule {
                name: "allow_all",
                description: None,
                check: |_record, _viewer| true,
            }],
            block: vec![],
            always_block: vec![],
        };

        let actor = Actor::Anonymous;
        let record = json!({});
        let result = PrivacyEvaluator::evaluate(&policy, &record, &actor);
        assert!(result.is_ok());
    }

    #[test]
    fn test_privacy_policy_evaluation_always_allow_overrides_block() {
        use serde_json::json;
        let policy = PrivacyPolicy {
            always_allow: vec![PrivacyRule {
                name: "always_allow",
                description: None,
                check: |_record, _viewer| true,
            }],
            allow: vec![],
            block: vec![PrivacyRule {
                name: "block",
                description: None,
                check: |_record, _viewer| true,
            }],
            always_block: vec![],
        };

        let actor = Actor::Anonymous;
        let record = json!({});
        let result = PrivacyEvaluator::evaluate(&policy, &record, &actor);
        assert!(result.is_ok());
    }

    #[test]
    fn test_privacy_policy_evaluation_block() {
        use serde_json::json;
        let policy = PrivacyPolicy {
            always_allow: vec![],
            allow: vec![],
            block: vec![PrivacyRule {
                name: "block",
                description: None,
                check: |_record, _viewer| true,
            }],
            always_block: vec![],
        };

        let actor = Actor::Anonymous;
        let record = json!({});
        let result = PrivacyEvaluator::evaluate(&policy, &record, &actor);
        assert!(result.is_err());
    }

    #[test]
    fn test_privacy_policy_evaluation_block_with_allow() {
        use serde_json::json;
        let policy = PrivacyPolicy {
            always_allow: vec![],
            allow: vec![PrivacyRule {
                name: "allow",
                description: None,
                check: |_record, _viewer| true,
            }],
            block: vec![PrivacyRule {
                name: "block",
                description: None,
                check: |_record, _viewer| true,
            }],
            always_block: vec![],
        };

        let actor = Actor::Anonymous;
        let record = json!({});
        let result = PrivacyEvaluator::evaluate(&policy, &record, &actor);
        assert!(result.is_err());
    }

    #[test]
    fn test_privacy_policy_evaluation_allow_without_block() {
        use serde_json::json;
        let policy = PrivacyPolicy {
            always_allow: vec![],
            allow: vec![PrivacyRule {
                name: "allow",
                description: None,
                check: |_record, _viewer| true,
            }],
            block: vec![PrivacyRule {
                name: "block",
                description: None,
                check: |_record, _viewer| false,
            }],
            always_block: vec![],
        };

        let actor = Actor::Anonymous;
        let record = json!({});
        let result = PrivacyEvaluator::evaluate(&policy, &record, &actor);
        assert!(result.is_ok());
    }

    #[test]
    fn test_privacy_policy_evaluation_no_policies() {
        use serde_json::json;
        let policy = PrivacyPolicy::default();

        let actor = Actor::Anonymous;
        let record = json!({});
        let result = PrivacyEvaluator::evaluate(&policy, &record, &actor);
        assert!(result.is_err(), "empty policies default-deny for non-System");

        let system = Actor::System {
            operation: "test".into(),
        };
        assert!(PrivacyEvaluator::evaluate(&policy, &record, &system).is_ok());
    }

    #[test]
    fn test_privacy_policy_evaluation_write() {
        use serde_json::json;
        let policy = PrivacyPolicy {
            always_allow: vec![],
            allow: vec![PrivacyRule {
                name: "allow",
                description: None,
                check: |_record, _viewer| true,
            }],
            block: vec![],
            always_block: vec![],
        };

        let actor = Actor::Anonymous;
        let record = json!({});
        let result = PrivacyEvaluator::evaluate(&policy, &record, &actor);
        assert!(result.is_ok());
    }
