use strict;
use warnings;
use feature qw(lexical_subs state);

my $top;

sub outer {
    my $x = shift;
    sub inner { return $x }     #~ warning nested-named-sub: `$x`
    sub declared;
    return inner();
}

sub loops {
    for my $item (1) {
        sub each_item { $item }    #~ warning nested-named-sub: `$item`
    }
}

my $code = sub {
    my $y;
    sub in_anon { $y }          #~ warning nested-named-sub: `$y`
};

# Nothing the outer sub makes on each call is read, so nothing is shared.
sub redeclared { my $x; sub again { my $x = 1; $x } }
sub constant_helper { sub answer { 42 } }
sub lexical { my $x; my sub inside { $x } inside() }
sub kept { state $n = 0; sub counter { $n++ } }
sub file_level { sub reads_top { $top } }
sub with_class {
    package Foo {
        sub bar { my ($self) = @_; $self }
    }
}

sub top { 1 }
BEGIN { sub in_begin { 1 } }
