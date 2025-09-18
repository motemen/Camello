use strict;
use warnings;

for (my $i = 0; $i < 100; $i++) {
    print $i, "\n";
}

my $undef = "";

warn ${^MATCH};

sub { "anon" };

sub foo {
    sub {}
}
