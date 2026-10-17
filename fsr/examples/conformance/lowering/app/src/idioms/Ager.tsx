import { Component } from "react";

export default class Ager extends Component<{ name: string }, { age: number }> {
  state = { age: 42 };

  handleClick = () => {
    this.setState({ age: this.state.age + 1 });
  };

  render() {
    return (
      <button className="ager" onClick={this.handleClick}>
        {this.props.name} is {this.state.age}
      </button>
    );
  }
}
