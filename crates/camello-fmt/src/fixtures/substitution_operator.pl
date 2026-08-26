# Substitution operator (s///) tests
# The replacement part should be verbatim unless 'e' flag is present

# Basic substitution with braces - preserve indentation in replacement
sub test_basic {
    s{
        pattern
    }{
        replacement
    }x;
}

# Substitution with different indentation levels
sub test_nested {
    s{
        A
    }{
        a
    }x;

    s{
        foo
    }{
            bar
    }x;

    s{
    pattern
    }{
replacement
    }x;
}

# Substitution with slashes
s/foo/bar/g;
s/pattern/replacement/;

# Substitution with parentheses
s(pattern)(replacement)g;

# Multiple substitutions
s{old}{new}g;
s{foo}{bar}i;
s{search}{replace}gi;

# Substitution with 'e' flag (code evaluation)
# Note: with the 'e' flag, the replacement is evaluated as code, not treated as verbatim
s{pattern}{uc($1)}e;
s{(\w+)}{lc($1)}ge;

# Complex whitespace preservation
s{
    # comment in pattern
    \s+
}{
    # spaces should be preserved
        replacement text
}x;

# Nested braces in pattern
s{
    \{
        content
    \}
}{
    result
}x;

# Various delimiter styles - formatting should be preserved
{
    s/foo/
        bar
    /;

    s{foo}{
        bar
    };

    s<foo><
        bar
    >;

    s#foo#
        bar
    #;
}

# The run ends where the scanner says it does: a `//` after the flags is
# defined-or, not a fourth delimiter (Devel::Cover::Collection).
my $version = $run->{version} =~ s/_//gr // next;
my $trimmed = $text =~ s{a}{b} // '';
my $matched = $text =~ m/a/g // 0;
my $literal = q{x} // 1;
