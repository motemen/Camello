given ($variable) {
    when ('foo') { say 'It was foo'; continue; }
    say 'This is not reached after foo';
}
