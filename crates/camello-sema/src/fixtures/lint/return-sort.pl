use strict;
use warnings;

sub a1 { return sort @_ }                  #~ warning return-sort
sub a2 { return sort { $a <=> $b } @_ }    #~ warning return-sort
sub a3 { return (sort @_) }                #~ warning return-sort
sub a4 { my @s = sort @_; return @s }
sub a5 { return wantarray ? sort @_ : undef }
