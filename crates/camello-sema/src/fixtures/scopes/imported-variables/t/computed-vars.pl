use strict;
use warnings;
my %vars = (low => [qw($digit)]);
use vars map {@$_} values %vars;

print $digit;
