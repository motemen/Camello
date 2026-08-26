# Whitespace between `<<` and the terminator, which perl allows as long as the
# terminator is quoted.
my $with_space = << "END";
body
END

my $dotted = << "    ...";
body
    ...
