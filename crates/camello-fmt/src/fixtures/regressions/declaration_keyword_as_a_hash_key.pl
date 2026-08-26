# `=>` quotes the bareword to its left, so a declaration keyword there is a
# string and not the start of a declaration.
my %record = (
    state => 'paid',
    local => 1,
    my    => 2,
    our   => 3,
);

my $ref = +{ state => 'pending' };
