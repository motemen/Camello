sub greet ($name, $greeting = "Hello") {
    "$greeting, $name!"
}

sub flexible ($first, $, $third //= 3, @rest) {
    $first + @rest;
}

my $anon = sub ($value ||= 10, %opts) {
    $opts{scale} ? $value * $opts{scale} : $value;
};

sub defaults ($value = $a || $b) {
    return $value;
}

sub placeholders ($, @, %) {
    return scalar @_;
}

sub slurpy ($head, @rest, %opts) {
    return ($head, scalar @rest, scalar keys %opts);
}

sub placeholder_default ($thing, $ = 1) {
    return $thing;
}

sub multiline (
    $first,
    $second = compute_default(),
    @rest,
) {
    return $first + @rest;
}
