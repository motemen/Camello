# `my $buf` inside a ternary: the `:` is the other arm, not an attribute list.
# AnyEvent::Handle writes this, and reading the colon as attributes swallowed
# the rest of the expression.
my $rbuf             = \($self->{tls} ? my $buf : $self->{rbuf});
my $pick             = $c ? my $a : my $b;
our $shared : shared = 1;
