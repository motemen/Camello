# A comment, then POD, are not code.

=head1 NAME

missing

=cut

package Foo;
use Exporter;
my $x = 1;  #~ warning missing-strict #~ warning missing-warnings
