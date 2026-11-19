use strict;
use warnings;

# `local our` declares as `our` does.
local our ($top, $depth);
print $top, $depth;

# `our` names the variable from where it is written.
our %Supported = (ascii => 1, %Supported);

# Braces end the name: this is `$m` and then text.
my $m = 'x';
print "${m}[0] ${m}{a}\n";

# In a pattern perl guesses; with only the scalar declared, it is the scalar.
my $pre = '(?<!\\\\)';
my $part = qr/\.[0-9]/;
my $input = '';
$input =~ s/$pre[\(\)]//g;
print 'v1.2.3' =~ /v1$part{2,}/;

# A key in single quotes interpolates nothing.
my $schema = {};
print "$schema->{'$ref'}\n";

print $nowhere;    #~ error undeclared-variable: `$nowhere`
