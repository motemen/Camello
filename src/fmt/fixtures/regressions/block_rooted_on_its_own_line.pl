my $x = $cond
    ? do {
        1;
    }
    : 2;

eval {
    foo();
}
    or do {
        bar();
    };

my %overload = (
    '+' => sub {
        my ($self, $other) = @_;
        $self->add($other);
    },
);

LOOP:
    while (1) {
        last LOOP;
    }

if ($a
    && $b) {
    c();
}

sub foo
    ($x) {
    1;
}
