use strict;
use warnings;

my $path = 'x';
open(my $a1, $path);  ## no critic (InputOutput::ProhibitTwoArgOpen)
open(my $a2, $path);  ## no critic (ProhibitTwoArgOpen)
open(my $a3, $path);  ## no critic (twoarg)
open(my $a4, $path);  ## no critic qw(ProhibitTwoArgOpen ProhibitStringyEval)
open(my $a5, $path);  ## no critic
open(my $a6, $path);  ## no critic (ProhibitStringyEval) #~ warning two-arg-open
open(FH1, $path);     ## no critic (TwoArgOpen) #~ warning bareword-filehandle

## no critic (ProhibitBarewordFileHandles)
open(FH2, '<', $path);
open(FH3, '<', $path);
## use critic
open(FH4, '<', $path);  #~ warning bareword-filehandle

sub scoped {
    ## no critic
    open(FH5, '<', $path);
    foo();;
}
open(FH6, '<', $path);  #~ warning bareword-filehandle
sub foo { 1 }
