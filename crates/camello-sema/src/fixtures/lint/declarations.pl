use strict;
use warnings;
use vars qw($legacy @legacy_list);

our $version = 1;
print "$version $legacy @legacy_list";

foreach my $item (1, 2) {
    print $item;
}

for (my $index = 0; $index < 3; $index++) {
    print $index;
}

if (my $found = 1) {
    print $found;
}

while (my ($key, $value) = each %ENV) {
    print "$key=$value";
}

eval {
    die "x";
};
if (my $error = $@) {
    print $error;
}

my $callback = sub {
    my ($argument) = @_;
    return $argument;
};
print $callback->(1);

sub takes_a_signature ($first, $second = 2, @rest) {
    return $first + $second + scalar @rest;
}

# `local` declares nothing, and names a package variable that this pass has
# no question about.
local $ENV{PATH} = '';
