#![cfg(not(target_arch = "wasm32"))]

#[cfg(test)]
mod tests {
    use ries_rs::expr::Expression;

    #[test]
    fn test_to_infix_basic() {
        // Parse expects postfix notation
        let expr = Expression::parse("x1+").unwrap();

        // DEBUG: Print the symbols to understand the structure
        println!("Parsed 'x1+':");
        println!("  Symbols slice: {:?}", expr.symbols());
        for (i, sym) in expr.symbols().iter().enumerate() {
            println!(
                "  [{}] {:?} seft={:?} display={}",
                i,
                sym,
                sym.seft(),
                sym.display_name()
            );
        }

        let infix = expr.to_infix();
        println!("  to_infix result: '{}'", infix);
        assert_eq!(infix, "x+1");
    }

    #[test]
    fn test_to_infix_postfix() {
        // Test with proper postfix notation: "x1+" instead of "x+1"
        let expr = Expression::parse("x1+").unwrap();
        println!("Parsed 'x1+':");
        println!("  Symbols slice: {:?}", expr.symbols());
        for (i, sym) in expr.symbols().iter().enumerate() {
            println!(
                "  [{}] {:?} seft={:?} display={}",
                i,
                sym,
                sym.seft(),
                sym.display_name()
            );
        }
        let infix = expr.to_infix();
        println!("  to_infix result: '{}'", infix);
        assert_eq!(infix, "x+1");
    }

    #[test]
    fn test_to_infix_complex() {
        // Test an expression with multiplication - postfix: "2x*"
        let expr = Expression::parse("2x*").unwrap();
        let infix = expr.to_infix();
        println!("  '2x*' -> '{}'", infix);
        assert!(infix.contains("2") && infix.contains("x"));
    }

    #[test]
    fn test_wasm_search_like() {
        // Mimic what WASM search does - generate expressions and convert them
        use ries_rs::gen::GenConfig;
        use ries_rs::search::{search_with_stats_and_config, SearchConfig};

        let target = 2.0;
        // Keep this test realistic but bounded: WASM level-0 search space and
        // early exit on an exact match.
        let (max_lhs, max_rhs) = ries_rs::search::level_to_complexity(0);
        let gen_config = GenConfig {
            max_lhs_complexity: max_lhs,
            max_rhs_complexity: max_rhs,
            ..GenConfig::default()
        };
        let search_config = SearchConfig {
            target,
            max_matches: 5,
            stop_at_exact: true,
            ..SearchConfig::default()
        };

        // Run search
        let (matches, _stats) = search_with_stats_and_config(&gen_config, &search_config);

        println!("Found {} matches", matches.len());

        // Try to convert each match to infix
        for (i, m) in matches.iter().enumerate().take(10) {
            println!("Match {}: LHS symbols: {:?}", i, m.lhs.expr.symbols());
            println!("  LHS infix: {}", m.lhs.expr.to_infix());
            println!("  RHS symbols: {:?}", m.rhs.expr.symbols());
            println!("  RHS infix: {}", m.rhs.expr.to_infix());
        }

        // Smoke test: search completes and any returned matches can be converted.
        // This path may legitimately return zero matches for some configs.
        assert!(matches.len() <= search_config.max_matches);
    }

    #[test]
    fn test_all_binary_operators() {
        use ries_rs::expr::Expression;
        use ries_rs::symbol::Symbol;

        // Test each Seft::C operator with a simple postfix expression
        let test_cases = vec![
            ("x1+", Symbol::Add),   // x + 1
            ("x1-", Symbol::Sub),   // x - 1
            ("x1*", Symbol::Mul),   // x * 1
            ("x1/", Symbol::Div),   // x / 1
            ("x1^", Symbol::Pow),   // x ^ 1
            ("x1v", Symbol::Root),  // x-th root of 1
            ("x1L", Symbol::Log),   // log base x of 1
            ("x1A", Symbol::Atan2), // atan2(x, 1)
        ];

        for (postfix, expected_sym) in test_cases {
            println!("Testing postfix: {}", postfix);
            let expr = Expression::parse(postfix).unwrap();
            println!("  Symbols: {:?}", expr.symbols());

            // Check that the expected symbol is present
            assert!(
                expr.symbols().contains(&expected_sym),
                "Expected symbol {:?} in expression {:?}",
                expected_sym,
                expr.symbols()
            );

            // Try to convert to infix
            let result = expr.to_infix();
            println!("  Infix: {}", result);
            assert!(!result.is_empty(), "Result should not be empty");
        }
    }
}
