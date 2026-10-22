# Contributing to this project

First off, I really thank you that you consider contributing to this project! It really means a lot and it is because of people like you who make open source such an amazing place to learn, create, experiment and create. A little by little efforts like people like you create massive changes that delivers value to this world. 

---

## Code  of Conduct

By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md). Please report unacceptable behaviour to the project maintainers.

---

## How to Contribute?

Here are the clear steps provided to contribute in this project.

### 1. Reporting Bugs
Before creating a bug report, please check existing issues if it has already been reported. When submitting a bug report, please include:
- A clear, brief title.
- Steps on how to reproduce the bug.
- Expected vs. actual behaviour.
- Environment details (OS, GLIBC version, runtime versions).

### 2. Suggesting Enhancements
Feature requests are tracked as GitHub issues. Please provide:
- A clear and brief title.
- A step-by-step explanation of the suggested feature.
- Examples of mockups if available.

---

## Development Workflow

Follow these steps to propose changes.

1. **Fork the Repository**
    
    Click the **Fork** button at the top right of this page to create a copy on your account.

<br>

2. **Clone your Fork**

    ```bash
   git clone https://github.com/YOUR-USERNAME/your-repo-name.git
   cd your-repo-name
    ```
<br>

3. **Create a Feature Branch**

    Create a branch off `main` with a descriptive name:

    ```bash
    git checkout -b feature/your-feature-name
    # or for bug fixes:
    git checkout -b fix/issue-description
    ```
<br>

4. **Install all required packages**
    
    Run the following command to download all required the packages. 

    ```bash
    # install packages
    npm install
    ```

<br>

5. **Make Your Changes & Test**

    Write your code and run tests to ensure existing functionality remains intact:

    ```bash
    npm run tauri dev
    ```

<br>

6. **Commit Your Changes**

    Write clear, concise commit messages:

    ```bash
    git commit -m "feat: add user authentication form validation"
    ```

<br>

7. **Push to GitHub**

    Push your changes to your fork:

    ```bash
    git push origin feature/your-feature-name
    ```

8. **Open a Pull Request (PR)**

    Navigate to the original repository on GitHub. You will see a banner asking to "**Compare & pull request**". Describe your changes and submit it for review!

## Commit Message Conventions

We recommend keeping commit messages clean and standardized:

`feat` : A new feature

`fix` : A bug fix

`docs` : Documentation-only changes

`style` : Formatting changes (spaces, semicolons, etc.)

`refactor` : Code changes that neither fix bugs nor add features

`test` : Adding or updating tests