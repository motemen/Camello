# `<...>` around anything other than a bare filehandle name or a scalar is a
# glob, equivalent to `glob EXPR`, not a readline.
my @files = <*.txt>;
my @more  = <lib/*.pm>;
my $line  = <STDIN>;
my $other = <$fh>;
