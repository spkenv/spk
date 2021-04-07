
#!/bin/bash
# test that a removed file is masked in future environments

filename="/spfs/message.txt";
base_tag="test/file_removal_base";
top_tag="test/file_removal_top";

spfs run - -- bash -c "echo hello > $filename && spfs commit layer -t $base_tag"
spfs run -e $base_tag -- bash -c "rm $filename && spfs commit platform -t $top_tag"
spfs run $top_tag -- test ! -f $filename
