# Basic named parameters with simple default
sub greet ($name, $greeting = "Hello") {
    "$greeting, $name!"
}

# Placeholders and slurpy parameters
sub flexible ($first, $, $third //= 3, @rest) {
    return $first + @rest;
}

# Hash slurpy with ||= default
sub configure ($debug ||= 0, %opts) {
    return $opts{level} // $debug;
}

# Explicit placeholders
sub placeholder_only ($, @, %) {
    return @_;
}

# Scalar placeholder alone
sub scalar_placeholder ($) {
    return 42;
}

# Placeholder with default value
sub placeholder_default ($thing, $ = 1) {
    return $thing + $;
}

# Complex default expression using logical or
sub complex_default ($value = $primary || $fallback) {
    return $value;
}

# Parenthesized low-precedence default
sub grouped_default ($name = (default_name() or "anon")) {
    return $name;
}

# Hash placeholder followed by named parameter
sub mixed_placeholders (%, $x) {
    return $x;
}

# Slurpy array at the end
sub slurpy_end ($first, @rest) {
    return $first + scalar @rest;
}

# Multiline signature with trailing comma
sub multiline (
    $alpha,
    $beta ||= compute_default(),
    %extra,
) {
    return $alpha + $beta + scalar keys %extra;
}

# Anonymous subroutine with signature
my $anon = sub ($value ||= 10, %opts) {
    return $opts{scale} ? $value * $opts{scale} : $value;
};
