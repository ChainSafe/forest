Actual file chunking

```
[root                              ]
[X         ][Y         ][X         ]
[a][b][a][a][b][c][d][a][a][b][a][a]
```

Car stream layout

```
1     2  3  4  5  6  7
(root)(X)[a][b](Y)[c][d]
```

How the link stack should look like

```
0 [root]
1 [X][Y][X]
2 [a][b][a][a][Y][a][b][a][a]
3 [-][b][a][a][Y][a][b][a][a]
4 [-][-][-][-][Y][a][b][a][a]
5 [-][-][-][-][-][c][d][a][a][b][a][a]
6 [-][-][-][-][-][-][d][a][a][b][a][a]
7 [-][-][-][-][-][-][-][-][-][-][-][-]
```
