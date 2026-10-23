# A `qw(...)` is placed like a bracket (INDENT-4): from the line it was opened
# on, not from the statement's level. Written as an argument hanging under a
# bareword call, or on a wrapped line, its contents go one level in from that
# line and its closing delimiter back to it — where `[` would put them.
frobnicate $x,
           qw(
               FOO
               BAR
           );

my $v = $cond
    ? [
        FOO
    ]
    : qw(
        BAR
    );
