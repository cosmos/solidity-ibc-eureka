package main

import (
	"encoding/json"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetGithubActionMatrixForTests(t *testing.T) {
	originalDir, err := os.Getwd()
	require.NoError(t, err)
	defer os.Chdir(originalDir)

	err = os.Chdir("../..")
	require.NoError(t, err)

	matrix, err := getGithubActionMatrixForTests("e2e/interchaintestv8", "", "", nil)
	require.NoError(t, err)

	assert.NotEmpty(t, matrix.Include, "Should discover tests")

	found := make(map[string]bool)
	for _, test := range matrix.Include {
		testKey := test.EntryPoint + "/" + test.Test
		found[testKey] = true
	}

	expectedTests := []string{
		"TestWithIbcEurekaTestSuite/Test_Deploy",
		"TestWithRelayerTestSuite/Test_2_ConcurrentRecvPacketToEth", 
		"TestWithSP1ICS07TendermintTestSuite/Test_UpdateClient",
	}

	for _, expected := range expectedTests {
		assert.True(t, found[expected], "Should find test: %s", expected)
	}
}

func TestFilterByTestName(t *testing.T) {
	originalDir, err := os.Getwd()
	require.NoError(t, err)
	defer os.Chdir(originalDir)

	err = os.Chdir("../..")
	require.NoError(t, err)

	matrix, err := getGithubActionMatrixForTests("e2e/interchaintestv8", "Test_UpdateClient", "", nil)
	require.NoError(t, err)

	assert.True(t, len(matrix.Include) >= 1, "Should have at least 1 test when filtering")
	
	hasCorrectTest := false
	for _, test := range matrix.Include {
		if test.Test == "Test_UpdateClient" {
			hasCorrectTest = true
			break
		}
	}
	assert.True(t, hasCorrectTest, "Should find the specific test we're looking for")
}

func TestFilterByExclusions(t *testing.T) {
	originalDir, err := os.Getwd()
	require.NoError(t, err)
	defer os.Chdir(originalDir)

	err = os.Chdir("../..")
	require.NoError(t, err)

	excludedSuites := []string{"TestWithRelayerTestSuite"}
	matrix, err := getGithubActionMatrixForTests("e2e/interchaintestv8", "", "", excludedSuites)
	require.NoError(t, err)

	for _, test := range matrix.Include {
		assert.NotEqual(t, "TestWithRelayerTestSuite", test.EntryPoint, "Should not contain excluded suite")
	}
}

func TestJSONOutput(t *testing.T) {
	testPairs := []TestSuitePair{
		{Test: "Test_Deploy", EntryPoint: "TestWithIbcEurekaTestSuite"},
		{Test: "Test_UpdateClient", EntryPoint: "TestWithSP1ICS07TendermintTestSuite"},
	}

	matrix := GithubActionTestMatrix{Include: testPairs}
	output, err := json.Marshal(matrix)
	require.NoError(t, err)

	var result GithubActionTestMatrix
	err = json.Unmarshal(output, &result)
	require.NoError(t, err)

	assert.Equal(t, testPairs, result.Include)
}

func TestIsTestSuiteMethod(t *testing.T) {
	tests := []struct {
		name     string
		code     string
		expected bool
	}{
		{
			name: "valid test suite method",
			code: `package main
import "testing"
func TestSomething(t *testing.T) {
	suite.Run(t, new(TestSuite))
}`,
			expected: true,
		},
		{
			name: "test method without suite.Run call",
			code: `package main
import "testing"
func TestSomething(t *testing.T) {
	t.Log("test")
}`,
			expected: false,
		},
		{
			name: "non-test function",
			code: `package main
import "testing"
func Something(t *testing.T) {
	suite.Run(t, new(TestSuite))
}`,
			expected: false,
		},
		{
			name: "test function with wrong parameter type",
			code: `package main
func TestSomething(t string) {
	suite.Run(t, new(TestSuite))
}`,
			expected: false,
		},
		{
			name: "test function with no parameters",
			code: `package main
func TestSomething() {
	suite.Run(nil, new(TestSuite))
}`,
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			fset := token.NewFileSet()
			file, err := parser.ParseFile(fset, "", tt.code, 0)
			require.NoError(t, err)

			var funcDecl *ast.FuncDecl
			for _, decl := range file.Decls {
				if f, ok := decl.(*ast.FuncDecl); ok && (f.Name.Name == "TestSomething" || f.Name.Name == "Something") {
					funcDecl = f
					break
				}
			}
			require.NotNil(t, funcDecl, "function not found")

			result := isTestSuiteMethod(funcDecl)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestIsTestFunction(t *testing.T) {
	tests := []struct {
		name     string
		code     string
		expected bool
	}{
		{
			name: "valid test method on test suite",
			code: `package main
type MyTestSuite struct{}
func (s *MyTestSuite) TestSomething() {}`,
			expected: true,
		},
		{
			name: "valid test method on suite",
			code: `package main
type MySuite struct{}
func (s *MySuite) TestSomething() {}`,
			expected: true,
		},
		{
			name: "non-test method on test suite",
			code: `package main
type MyTestSuite struct{}
func (s *MyTestSuite) Something() {}`,
			expected: false,
		},
		{
			name: "test function without receiver",
			code: `package main
func TestSomething() {}`,
			expected: false,
		},
		{
			name: "test method on non-suite type",
			code: `package main
type MyStruct struct{}
func (s *MyStruct) TestSomething() {}`,
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			fset := token.NewFileSet()
			file, err := parser.ParseFile(fset, "", tt.code, 0)
			require.NoError(t, err)

			var funcDecl *ast.FuncDecl
			for _, decl := range file.Decls {
				if f, ok := decl.(*ast.FuncDecl); ok && (f.Name.Name == "TestSomething" || f.Name.Name == "Something") {
					funcDecl = f
					break
				}
			}
			require.NotNil(t, funcDecl, "function not found")

			result := isTestFunction(funcDecl)
			assert.Equal(t, tt.expected, result)
		})
	}
}