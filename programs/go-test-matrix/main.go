package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strings"
)

const (
	testNamePrefix     = "Test"
	testFileNameSuffix = "_test.go"
	e2eTestDirectory   = "e2e/interchaintestv8"
	testEntryPointEnv  = "TEST_ENTRYPOINT"
	testExclusionsEnv  = "TEST_EXCLUSIONS"
	testNameEnv        = "TEST_NAME"
)

type GithubActionTestMatrix struct {
	Include []TestSuitePair `json:"include"`
}

type TestSuitePair struct {
	Test       string `json:"test"`
	EntryPoint string `json:"entrypoint"`
}

func main() {
	githubActionMatrix, err := getGithubActionMatrixForTests(e2eTestDirectory, getTestToRun(), getTestEntrypointToRun(), getExcludedTestFunctions())
	if err != nil {
		fmt.Printf("error generating github action json: %s", err)
		os.Exit(1)
	}

	ghBytes, err := json.Marshal(githubActionMatrix)
	if err != nil {
		fmt.Printf("error marshalling github action json: %s", err)
		os.Exit(1)
	}
	fmt.Println(string(ghBytes))
}

func getTestEntrypointToRun() string {
	testSuite, ok := os.LookupEnv(testEntryPointEnv)
	if !ok {
		return ""
	}
	return testSuite
}

func getTestToRun() string {
	testName, ok := os.LookupEnv(testNameEnv)
	if !ok {
		return ""
	}
	return testName
}

func getExcludedTestFunctions() []string {
	exclusions, ok := os.LookupEnv(testExclusionsEnv)
	if !ok {
		return nil
	}
	return strings.Split(exclusions, ",")
}

func getGithubActionMatrixForTests(e2eRootDirectory, testName string, suite string, excludedItems []string) (GithubActionTestMatrix, error) {
	testSuiteMapping := map[string][]string{}
	fset := token.NewFileSet()
	err := filepath.Walk(e2eRootDirectory, func(path string, info fs.FileInfo, err error) error {
		if err != nil {
			return fmt.Errorf("error walking e2e directory: %w", err)
		}

		if !strings.HasSuffix(path, testFileNameSuffix) {
			return nil
		}

		f, err := parser.ParseFile(fset, path, nil, 0)
		if err != nil {
			return fmt.Errorf("failed parsing file: %w", err)
		}

		suiteNameForFile, testCases, err := extractSuiteAndTestNames(f)
		if err != nil {
			return nil
		}

		if testName != "" && slices.Contains(testCases, testName) {
			testCases = []string{testName}
		}

		if slices.Contains(excludedItems, suiteNameForFile) {
			return nil
		}

		if suite == "" || suiteNameForFile == suite {
			testSuiteMapping[suiteNameForFile] = testCases
		}

		return nil
	})
	if err != nil {
		return GithubActionTestMatrix{}, err
	}

	gh := GithubActionTestMatrix{
		Include: []TestSuitePair{},
	}

	for testSuiteName, testCases := range testSuiteMapping {
		for _, testCaseName := range testCases {
			gh.Include = append(gh.Include, TestSuitePair{
				Test:       testCaseName,
				EntryPoint: testSuiteName,
			})
		}
	}

	if len(gh.Include) == 0 {
		return GithubActionTestMatrix{}, errors.New("no test cases found")
	}

	sort.SliceStable(gh.Include, func(i, j int) bool {
		return gh.Include[i].Test < gh.Include[j].Test
	})


	return gh, nil
}

func extractSuiteAndTestNames(file *ast.File) (string, []string, error) {
	var suiteNameForFile string
	var testCases []string

	for _, d := range file.Decls {
		if f, ok := d.(*ast.FuncDecl); ok {
			functionName := f.Name.Name
			if isTestSuiteMethod(f) {
				if suiteNameForFile != "" {
					return "", nil, fmt.Errorf("found a second test function: %s when %s was already found", f.Name.Name, suiteNameForFile)
				}
				suiteNameForFile = functionName
				continue
			}
			if isTestFunction(f) {
				testCases = append(testCases, functionName)
			}
		}
	}
	if suiteNameForFile == "" {
		return "", nil, fmt.Errorf("file %s had no test suite test case", file.Name.Name)
	}
	return suiteNameForFile, testCases, nil
}

func isTestSuiteMethod(f *ast.FuncDecl) bool {
	if !strings.HasPrefix(f.Name.Name, testNamePrefix) || len(f.Type.Params.List) != 1 {
		return false
	}
	
	param := f.Type.Params.List[0]
	if len(param.Names) != 1 {
		return false
	}
	
	if starExpr, ok := param.Type.(*ast.StarExpr); ok {
		if selectorExpr, ok := starExpr.X.(*ast.SelectorExpr); ok {
			if ident, ok := selectorExpr.X.(*ast.Ident); ok && ident.Name == "testing" && selectorExpr.Sel.Name == "T" {
				return containsSuiteRunCall(f)
			}
		}
	}
	
	return false
}

func isTestFunction(f *ast.FuncDecl) bool {
	if !strings.HasPrefix(f.Name.Name, testNamePrefix) || f.Recv == nil || len(f.Recv.List) != 1 {
		return false
	}
	
	receiver := f.Recv.List[0]
	if starExpr, ok := receiver.Type.(*ast.StarExpr); ok {
		if ident, ok := starExpr.X.(*ast.Ident); ok {
			return strings.HasSuffix(ident.Name, "TestSuite") || strings.HasSuffix(ident.Name, "Suite")
		}
	}
	
	return false
}

func containsSuiteRunCall(f *ast.FuncDecl) bool {
	if f.Body == nil {
		return false
	}
	
	for _, stmt := range f.Body.List {
		if exprStmt, ok := stmt.(*ast.ExprStmt); ok {
			if callExpr, ok := exprStmt.X.(*ast.CallExpr); ok {
				if selectorExpr, ok := callExpr.Fun.(*ast.SelectorExpr); ok {
					if ident, ok := selectorExpr.X.(*ast.Ident); ok && ident.Name == "suite" && selectorExpr.Sel.Name == "Run" {
						return true
					}
				}
			}
		}
	}
	
	return false
}


