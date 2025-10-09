print STDOUT"hello";
print(STDOUT"world");
print   STDOUT    "line\n";

map {$_}@list;
map({$_}@list);

grep{$_}@list;
grep({$_}@list);
