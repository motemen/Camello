use strict;
use warnings;

sub cached {
    my ($self, $x) = @_;
    $self->{cache} //= do { return $x if $x; compute() };  #~ warning return-in-do-value
    my $v = do { return unless $self; compute() };          #~ warning return-in-do-value
    my @list = (1, do { return 2 });                        #~ warning return-in-do-value
    print do { return 3 };                                  #~ warning return-in-do-value
    my $t = $x ? do { return 4 } : 5;                       #~ warning return-in-do-value
    return do { return 6 if $x; 7 };                        #~ warning return-in-do-value
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
    my $u = do { 1 };
    return $v + $w + $u;
}

sub compute { 1 }
sub ok { 1 }
sub next_one { 0 }
