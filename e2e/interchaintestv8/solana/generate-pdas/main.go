package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

const (
	seedKindConst   = "const"
	seedKindArg     = "arg"
	seedKindAccount = "account"
)

// Configuration holds the command-line configuration
type Configuration struct {
	IDLDirectory string
	OutputFile   string
}

// IDL Types - Domain models for Anchor IDL structure
type IDL struct {
	Address      string        `json:"address"`
	Metadata     Metadata      `json:"metadata"`
	Instructions []Instruction `json:"instructions"`
}

type Metadata struct {
	Name string `json:"name"`
}

type Instruction struct {
	Name     string    `json:"name"`
	Accounts []Account `json:"accounts"`
}

type Account struct {
	Name string  `json:"name"`
	PDA  *PDADef `json:"pda,omitempty"`
}

type PDADef struct {
	Seeds []Seed `json:"seeds"`
}

type Seed struct {
	Kind  string `json:"kind"`
	Value []byte `json:"value,omitempty"`
	Path  string `json:"path,omitempty"`
}

// PDAPattern represents a unique PDA pattern to generate
type PDAPattern struct {
	Name        string
	FuncName    string
	Seeds       []Seed
	ProgramName string
	ProgramID   string
}

// Generator handles the PDA generation process
type Generator struct {
	config   *Configuration
	patterns []PDAPattern
}

// NewGenerator creates a new Generator instance
func NewGenerator(config *Configuration) *Generator {
	return &Generator{
		config: config,
	}
}

// Run executes the complete generation process
func (g *Generator) Run() error {
	// Extract PDA patterns from IDL files
	if err := g.extractPatterns(); err != nil {
		return fmt.Errorf("extracting patterns: %w", err)
	}

	// Generate Go code
	code, err := g.generateCode()
	if err != nil {
		return fmt.Errorf("generating code: %w", err)
	}

	// Write output file
	if err := os.WriteFile(g.config.OutputFile, []byte(code), 0o600); err != nil {
		return fmt.Errorf("writing output: %w", err)
	}

	fmt.Printf("Generated %d PDA helpers to %s\n", len(g.patterns), g.config.OutputFile)
	return nil
}

// extractPatterns reads all IDL files and extracts unique PDA patterns
func (g *Generator) extractPatterns() error {
	files, err := g.findIDLFiles()
	if err != nil {
		return err
	}

	patterns := make([]PDAPattern, 0)
	seenSignatures := make(map[string]bool)

	for _, file := range files {
		filePatterns, err := g.extractFromFile(file)
		if err != nil {
			return fmt.Errorf("processing %s: %w", file, err)
		}

		for _, pattern := range filePatterns {
			signature := pattern.buildSignature()
			if !seenSignatures[signature] {
				seenSignatures[signature] = true
				pattern.FuncName = pattern.buildFuncName()
				patterns = append(patterns, pattern)
			}
		}
	}

	// Sort patterns by function name for consistent output
	sort.Slice(patterns, func(i, j int) bool {
		return patterns[i].FuncName < patterns[j].FuncName
	})

	g.patterns = patterns
	return nil
}

// findIDLFiles discovers all JSON files in the IDL directory
func (g *Generator) findIDLFiles() ([]string, error) {
	entries, err := os.ReadDir(g.config.IDLDirectory)
	if err != nil {
		return nil, fmt.Errorf("reading IDL directory: %w", err)
	}

	var files []string
	for _, entry := range entries {
		if !entry.IsDir() && strings.HasSuffix(entry.Name(), ".json") {
			files = append(files, filepath.Join(g.config.IDLDirectory, entry.Name()))
		}
	}

	if len(files) == 0 {
		return nil, fmt.Errorf("no IDL JSON files found in %s", g.config.IDLDirectory)
	}

	return files, nil
}

// extractFromFile extracts PDA patterns from a single IDL file
func (g *Generator) extractFromFile(path string) ([]PDAPattern, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("reading file: %w", err)
	}

	var idl IDL
	if err := json.Unmarshal(data, &idl); err != nil {
		return nil, fmt.Errorf("parsing JSON: %w", err)
	}

	programName := toPascalCase(idl.Metadata.Name)
	var patterns []PDAPattern

	for _, instruction := range idl.Instructions {
		for _, account := range instruction.Accounts {
			if account.PDA != nil {
				patterns = append(patterns, PDAPattern{
					Name:        account.Name,
					Seeds:       account.PDA.Seeds,
					ProgramName: programName,
					ProgramID:   idl.Address,
				})
			}
		}
	}

	return patterns, nil
}

// buildSignature creates a unique signature for deduplication
func (p *PDAPattern) buildSignature() string {
	var parts []string
	parts = append(parts, p.ProgramName)

	for _, seed := range p.Seeds {
		if seed.Kind == seedKindConst {
			parts = append(parts, string(seed.Value))
		} else {
			parts = append(parts, seed.Kind)
		}
	}

	return strings.Join(parts, "|")
}

// buildFuncName generates the function name for this PDA pattern
func (p *PDAPattern) buildFuncName() string {
	builder := &funcNameBuilder{
		pattern: p,
	}
	return builder.build()
}

// funcNameBuilder helps construct function names
type funcNameBuilder struct {
	pattern *PDAPattern
}

func (b *funcNameBuilder) build() string {
	nameParts := b.extractNameParts()
	baseName := b.pattern.ProgramName + strings.Join(nameParts, "")

	if b.hasAccountSeed() {
		baseName += "WithAccountSeed"
	}

	return baseName + "PDA"
}

func (b *funcNameBuilder) extractNameParts() []string {
	var parts []string

	for _, seed := range b.pattern.Seeds {
		if seed.Kind == seedKindConst {
			parts = append(parts, toPascalCase(string(seed.Value)))
		}
	}

	// Use account name if no const seeds found
	if len(parts) == 0 {
		parts = append(parts, toPascalCase(b.pattern.Name))
	}

	return parts
}

func (b *funcNameBuilder) hasAccountSeed() bool {
	for _, seed := range b.pattern.Seeds {
		if seed.Kind == seedKindAccount {
			return true
		}
	}
	return false
}

// CodeGenerator handles code generation
type CodeGenerator struct {
	patterns []PDAPattern
}

// generateCode creates the Go source code
func (g *Generator) generateCode() (string, error) {
	cg := &CodeGenerator{patterns: g.patterns}
	return cg.generate()
}

func (cg *CodeGenerator) generate() (string, error) {
	var b strings.Builder

	// Write file header
	b.WriteString(cg.generateHeader())

	// Generate each function
	for _, pattern := range cg.patterns {
		b.WriteString(cg.generateFunction(pattern))
		b.WriteString("\n")
	}

	return b.String(), nil
}

func (cg *CodeGenerator) generateHeader() string {
	return `// Code generated by tools/generate-pdas. DO NOT EDIT.
//
// This file is automatically generated from Anchor IDL files.
// Run 'just generate-pda' to regenerate.
//
// DO NOT EDIT THIS FILE MANUALLY.

package solana

import (
	"fmt"

	solanago "github.com/gagliardetto/solana-go"
)

`
}

func (cg *CodeGenerator) generateFunction(p PDAPattern) string {
	fg := &functionGenerator{pattern: p}
	return fg.generate()
}

// functionGenerator generates a single PDA function
type functionGenerator struct {
	pattern PDAPattern
}

func (fg *functionGenerator) generate() string {
	var b strings.Builder

	// Function signature
	b.WriteString(fg.generateSignature())
	b.WriteString(" {\n")

	// Seeds array
	b.WriteString(fg.generateSeedsArray())

	// PDA derivation
	b.WriteString(fg.generatePDADerivation())

	// Error handling
	b.WriteString(fg.generateErrorHandling())

	// Return statement
	b.WriteString("\treturn pda, bump\n")
	b.WriteString("}\n")

	return b.String()
}

func (fg *functionGenerator) generateSignature() string {
	params := fg.extractParameters()
	return fmt.Sprintf("func %s(%s) (solanago.PublicKey, uint8)",
		fg.pattern.FuncName, params)
}

func (fg *functionGenerator) extractParameters() string {
	params := []string{"programID solanago.PublicKey"}
	seen := make(map[string]bool)

	for _, seed := range fg.pattern.Seeds {
		if seed.Kind == seedKindArg || seed.Kind == seedKindAccount {
			paramName := extractParamName(seed.Path)
			paramKey := fmt.Sprintf("%s_%s", seed.Kind, paramName)

			if !seen[paramKey] {
				params = append(params, fmt.Sprintf("%s []byte", paramName))
				seen[paramKey] = true
			}
		}
	}

	return strings.Join(params, ", ")
}

func (fg *functionGenerator) generateSeedsArray() string {
	seedsCode := fg.buildSeedsCode()
	return fmt.Sprintf("\tpda, bump, err := solanago.FindProgramAddress(\n\t\t[][]byte{%s},\n\t\tprogramID,\n\t)\n",
		strings.Join(seedsCode, ", "))
}

func (fg *functionGenerator) buildSeedsCode() []string {
	var seeds []string

	for _, seed := range fg.pattern.Seeds {
		switch seed.Kind {
		case seedKindConst:
			seeds = append(seeds, fmt.Sprintf("[]byte(\"%s\")", string(seed.Value)))
		case seedKindArg, seedKindAccount:
			seeds = append(seeds, extractParamName(seed.Path))
		}
	}

	return seeds
}

func (fg *functionGenerator) generatePDADerivation() string {
	return ""
}

func (fg *functionGenerator) generateErrorHandling() string {
	return fmt.Sprintf("\tif err != nil {\n\t\tpanic(fmt.Sprintf(\"failed to derive %s PDA: %%v\", err))\n\t}\n",
		fg.pattern.FuncName)
}

// String manipulation utilities
func toPascalCase(s string) string {
	s = strings.ReplaceAll(s, "-", "_")
	parts := strings.Split(s, "_")

	for i, part := range parts {
		if len(part) > 0 {
			parts[i] = strings.ToUpper(part[:1]) + strings.ToLower(part[1:])
		}
	}

	return strings.Join(parts, "")
}

func toCamelCase(s string) string {
	parts := strings.Split(s, "_")

	for i := 1; i < len(parts); i++ {
		if len(parts[i]) > 0 {
			parts[i] = strings.ToUpper(parts[i][:1]) + parts[i][1:]
		}
	}

	return strings.Join(parts, "")
}

func extractParamName(path string) string {
	parts := strings.Split(path, ".")
	name := parts[len(parts)-1]
	return toCamelCase(name)
}

// parseConfiguration parses command-line arguments
func parseConfiguration() (*Configuration, error) {
	var config Configuration

	flag.StringVar(&config.IDLDirectory, "idl-dir", "", "Directory containing IDL JSON files")
	flag.StringVar(&config.OutputFile, "output", "", "Output Go file")
	flag.Parse()

	if config.IDLDirectory == "" {
		return nil, fmt.Errorf("--idl-dir is required")
	}

	if config.OutputFile == "" {
		return nil, fmt.Errorf("--output is required")
	}

	// Validate IDL directory exists
	info, err := os.Stat(config.IDLDirectory)
	if err != nil {
		return nil, fmt.Errorf("IDL directory error: %w", err)
	}
	if !info.IsDir() {
		return nil, fmt.Errorf("IDL path is not a directory: %s", config.IDLDirectory)
	}

	return &config, nil
}

func main() {
	config, err := parseConfiguration()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}

	generator := NewGenerator(config)
	if err := generator.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "Generation failed: %v\n", err)
		os.Exit(1)
	}
}
