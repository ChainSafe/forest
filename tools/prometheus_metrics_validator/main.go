package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/prometheus/prometheus/model/textparse"
	"github.com/urfave/cli/v3"
)

func main() {
	cmd := &cli.Command{
		Name:  "prometheus-metrics-validator",
		Usage: "A tool for validating the compatibility of Prometheus metrics text files with Grafana scrapers.",
		Arguments: []cli.Argument{
			&cli.StringArg{
				Name:      "file",
				UsageText: "Path to the Prometheus metrics text file",
			},
		},
		Action: func(ctx context.Context, c *cli.Command) error {
			path := c.StringArg("file")
			metrics, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			if err = Validate(metrics); err != nil {
				fmt.Printf("Validation failed: %v\n", err)
				os.Exit(1)
			} else {
				fmt.Println("Validation passed!")
			}
			return nil
		},
	}
	if err := cmd.Run(context.Background(), os.Args); err != nil {
		panic(err)
	}
}

func Validate(metrics []byte) error {
	p, err := textparse.New(metrics, "text/plain", nil, textparse.ParserOptions{EnableTypeAndUnitLabels: true, ConvertClassicHistogramsToNHCB: true})
	if err != nil {
		return err
	}
	for {
		if _, err := p.Next(); err != nil {
			if errors.Is(err, io.EOF) {
				break
			} else {
				return err
			}
		}
	}
	return nil
}
