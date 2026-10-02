    #[test]
    fn test_filter_entity_fields_public() {
        use crate::actor::Actor;
        use serde_json::json;

        let schema = schema_with_fields(vec![
            SchemaField {
                name: "id".to_string(),
                field_type: "string".to_string(),
                primary: true,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: Some(policies_with_read_allow(&["PUBLIC_READ"])),
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            },
            SchemaField {
                name: "name".to_string(),
                field_type: "string".to_string(),
                primary: false,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: Some(policies_with_read_allow(&["PUBLIC_READ"])),
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            },
            SchemaField {
                name: "email".to_string(),
                field_type: "string".to_string(),
                primary: false,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: Some(policies_with_read_allow(&["OWNER_BY_ID"])),
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            },
            SchemaField {
                name: "secret".to_string(),
                field_type: "string".to_string(),
                primary: false,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: Some(policies_with_read_allow(&["SYSTEM_ONLY"])),
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            },
        ]);

        let raw_data = json!({
            "id": "123",
            "name": "Test User",
            "email": "test@example.com",
            "secret": "hidden"
        });

        let actor = Actor::Anonymous;

        let (filtered, hidden) =
            PrivacyEvaluator::filter_entity_fields(&schema, &raw_data, &actor).unwrap();

        assert!(filtered.contains_key("id"));
        assert!(filtered.contains_key("name"));
        assert!(!filtered.contains_key("email"));
        assert!(!filtered.contains_key("secret"));

        assert!(hidden.contains(&"email".to_string()));
        assert!(hidden.contains(&"secret".to_string()));
    }

    #[test]
    fn test_filter_entity_fields_owner() {
        use crate::actor::Actor;
        use serde_json::json;

        let schema = schema_with_fields(vec![
            SchemaField {
                name: "id".to_string(),
                field_type: "string".to_string(),
                primary: true,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: Some(policies_with_read_allow(&["PUBLIC_READ"])),
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            },
            SchemaField {
                name: "email".to_string(),
                field_type: "string".to_string(),
                primary: false,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: Some(policies_with_read_allow(&["OWNER_BY_ID"])),
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            },
            SchemaField {
                name: "secret".to_string(),
                field_type: "string".to_string(),
                primary: false,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: Some(policies_with_read_allow(&["SYSTEM_ONLY"])),
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            },
        ]);

        let raw_data = json!({
            "id": "user123",
            "email": "test@example.com",
            "secret": "hidden"
        });

        let actor = Actor::User {
            user_id: "user123".to_string(),
        };

        let (filtered, hidden) =
            PrivacyEvaluator::filter_entity_fields(&schema, &raw_data, &actor).unwrap();

        assert!(filtered.contains_key("email"));
        assert!(!filtered.contains_key("secret"));
        assert!(hidden.contains(&"secret".to_string()));
    }

    #[test]
    fn test_filter_entity_fields_system() {
        use crate::actor::Actor;
        use serde_json::json;

        let schema = schema_with_fields(vec![
            SchemaField {
                name: "id".to_string(),
                field_type: "string".to_string(),
                primary: true,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: Some(policies_with_read_allow(&["PUBLIC_READ"])),
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            },
            SchemaField {
                name: "secret".to_string(),
                field_type: "string".to_string(),
                primary: false,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: Some(policies_with_read_allow(&["SYSTEM_ONLY"])),
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            },
        ]);

        let raw_data = json!({
            "id": "123",
            "secret": "hidden"
        });

        let actor = Actor::System {
            operation: "test".to_string(),
        };

        let (filtered, hidden) =
            PrivacyEvaluator::filter_entity_fields(&schema, &raw_data, &actor).unwrap();

        assert!(filtered.contains_key("id"));
        assert!(filtered.contains_key("secret"));
        assert_eq!(hidden, Vec::<String>::new());
    }
