use strict;
use warnings;
use List::Util qw(first);

sub cached {
    my ($self, $x) = @_;
    $self->{cache} //= do { return $x if $x; compute() };  #~ warning return-in-do-value
    my $v = do { return unless $self; compute() };          #~ warning return-in-do-value
    my @list = (1, do { return 2 });                        #~ warning return-in-do-value
    print do { return 3 };                                  #~ warning return-in-do-value
    my $t = $x ? do { return 4 } : 5;                       #~ warning return-in-do-value
    return do { return 6 if $x; 7 };                        #~ warning return-in-do-value
}

sub both {
    my ($cache) = @_;
    $cache //= do {
        my $r = compute();
        return $r if $r;    #~ warning return-in-do-value
        return '';          #~ warning return-in-do-value
    };
}

# A `return` in a branch or a loop inside the `do` is an early exit the
# branch was written for (LINT-1a).
sub branches {
    my ($ok, @a) = @_;
    my $v = do {
        if   ($ok) { compute() }
        else       { return (undef, 'error') }
    };
    my $w = do { for (@a) { return $_ if $_ } 0 };
    my $b = do { { return 1 if $ok } 2 };
    return $v + $w + $b;
}

sub last_statement {
    do { return 1 if $_[0]; 2 }                             #~ warning return-in-do-value
}

sub early {
    my ($f) = @_;
    open my $fh, '<', $f or do { warn $!; return };
    defined $f && do { ok($f) or return };
    $f || do { return };
    do { return 1 if $f };
    do { return if $f; next_one() } while ($f);
    my $v = do { my $s = sub { return 1 }; $s->() };
    my $w = do { eval { return 1 } };
    my $z = do { first { return 1 } ($f) };
    my $y = do { scraper { return 1 } };
    my $s = do { scraper { my $n = 1; return $n } };
    my $m = do { map { return 1 } ($f) };
    my $u = do { 1 };
    return $v + $w + $u;
}

sub nested_do {
    my $t = do { 1; do { return 3 } };                      #~ warning return-in-do-value
    return $t;
}

sub branch_ends_the_sub {
    if ($_[0]) {
        do { return 2 }                                     #~ warning return-in-do-value
    }
}

sub branch_then_more {
    if ($_[0]) {
        do { return 2 }
    }
    1;
}

sub compute { 1 }
sub ok { 1 }
sub next_one { 0 }
sub scraper { 1 }
