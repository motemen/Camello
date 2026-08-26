# `<...>` around anything other than a bare filehandle name or a scalar is a
# glob, equivalent to `glob EXPR`, not a readline. A space straight after the
# `<` is what still makes it a comparison.
my @files = <*.txt>;
my @more  = <lib/*.pm>;
my @nodes = <example:name />;
my $line  = <STDIN>;
my $other = <$fh>;
my $cmp   = f < $x > 1;
