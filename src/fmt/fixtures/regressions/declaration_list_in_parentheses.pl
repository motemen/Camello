# `my (...)` holds its list in parentheses like any other bracket, and the
# parentheses are what tells it from `my $x`. Read as an ordinary sequence they
# seeded no break, so a declaration list the writer opened on a line of its own
# came back hung under the `my`, with the `)` on the last element's line.
my (
    $first,
    $second,
);

our (
    # INITIALIZER: BEGIN block
    $config,
    $debug,
);

# The writer put something after the `(`, so the brackets stay flat and keep the
# lines they were given (INDENT-2) — and the `)` written on a line of its own
# stays on one.
our ($st_dev, $st_ino, $st_mode,
    $st_nlink, $st_uid, $st_gid
);

my ($self, $args) = @_;

my $plain = 1;
