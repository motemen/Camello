use camello::{
    format_with_options, parse_perl, DelimiterTightness, DelimiterTightnessConfig, FormatterOptions,
};

#[test]
fn loose_spacing_for_ref_accesses() {
    let source = "$h->{$o->meth};\n$a->[$x+$y];\n";
    let (syntax, errors) = parse_perl(source);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors);

    let options = FormatterOptions::default().with_delimiter_tightness(
        DelimiterTightnessConfig::default()
            .with_braces(DelimiterTightness::Loose)
            .with_brackets(DelimiterTightness::Loose),
    );

    let formatted = format_with_options(&syntax, options);
    assert_eq!(formatted, "$h->{ $o->meth };\n$a->[ $x + $y ];\n");
}
