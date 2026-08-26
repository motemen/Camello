sub custom { $a <=> $b }
my @values;
my $cmp = \&custom;

my @sorted = sort $cmp @values;
my @other  = sort custom @values;
