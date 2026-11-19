use strict;
use warnings;

sub outer {
    my $x = shift;
    sub inner { return $x }     #~ warning nested-named-sub
    sub declared;
    return inner();
}

my $code = sub {
    sub in_anon { 1 }           #~ warning nested-named-sub
};

sub top { 1 }
BEGIN { sub in_begin { 1 } }
