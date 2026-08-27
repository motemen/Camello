# A signature is a bracketed list like any other, so a newline at its seed
# breaks it and gives each parameter a line (INDENT-2). The parameters used to
# be direct children of the signature, out of the bracket rule's reach, and a
# signature the writer opened on a line of its own came back hung under the
# subroutine's name with its `)` on the last parameter's line.
sub multiline (
    $alpha,
    $beta ||= compute_default(),
    %extra,
) {
    return $alpha;
}

# No newline at the seed, so the brackets stay flat and keep the writer's lines.
sub wrapped ($first,
    $second) {
    return $first;
}

sub inline ($name, $greeting = "Hello") {
    return "$greeting, $name";
}

sub placeholders ($, @) {
    return @_;
}

# A prototype is raw text and none of this reaches it.
sub prototyped ($$) {
    return 1;
}

my $anon = sub (
    $value,
    %opts,
) {
    return $value;
};
