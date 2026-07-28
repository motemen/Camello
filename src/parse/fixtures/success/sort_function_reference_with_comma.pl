my @sorted = sort $cmp, @values;
my @other  = sort \&custom, @values;
