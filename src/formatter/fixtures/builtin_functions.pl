print STDOUT"hello";
print(STDOUT"world");
print   STDOUT    "line\n";

map {$_}@list;
map({$_}@list);
map {$_}(1,2,3);
map {$_}[1,2,3];

grep{$_}@list;
grep({$_}@list);

print { $fh } "hello";

each%$hash;
each @array;
values%$hash;
values @array;
