package My::Computed;
use strict;
use warnings;
use Exporter 'import';
my %groups = (low => [qw($digit $letter)]);
our @EXPORT_OK   = map {@$_} values %groups;
our %EXPORT_TAGS = (%groups, ALL => [@EXPORT_OK]);
1;
