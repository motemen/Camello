# Without `use strict` an undeclared name is a package variable and a legal
# program, so there is nothing here to report.
$counter = 0;
$counter++;
print "$counter $whatever";
