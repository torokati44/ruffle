temci short exec -d "my_description" "python3 ./run.py" --runner output --cpuset_base_core_number 4 --runs 4 --append --sudo  --preset usable
temci report --reporter html2 # then type True if asked
