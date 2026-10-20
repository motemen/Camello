# A `qw(...)` written over several lines is indented like a bracket (INDENT-2):
# its contents one level in, its closing delimiter back on the `qw` line's
# column. The contents are one QW_STRING token, so the formatter passed it
# through as written, flush left or not (#386).
our @EXPORT_OK = qw(
    FOO
    BAR
    BAZ
);

sub list {
    my @x = qw(
        FOO BAR
        BAZ
    );
    return \@x;
}

sub deeper {
    my @x = qw(
        FOO
        BAR
    );
    return \@x;
}

# No line break after the opening delimiter: the lines after the first take
# one level, as a bracket's do.
our @EXPORT = qw(FOO BAR
    BAZ);
