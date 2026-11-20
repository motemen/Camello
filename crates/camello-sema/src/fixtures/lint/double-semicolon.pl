use strict;
use warnings;

foo();;                  #~ warning double-semicolon
my $x = 1;;;             #~ warning double-semicolon #~ warning double-semicolon
{ foo() };
sub foo { 1 };
if ($x) { foo() };
package Bar { sub baz { 1 } };
for (;;) { last }
my $s = sub { 1 };
