sub named { my $x = shift; $x + 1 }
my $anon = sub { setup(); teardown() };
my $with_semicolon = sub { setup(); teardown(); };
do { local $/; <$fh> };
map { $_ = '..' if $_ eq '-'; $_ } @dirs;
eval { local $SIG{__DIE__}; require $file };
sub nested_block_that_breaks { setup(); sub { inner(); } }
