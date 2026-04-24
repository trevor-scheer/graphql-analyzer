import { gql } from "@apollo/client";

const GET_USER = gql`
  query {
    user(id: "1") {
      name
    }
  }
`;

export function UserComponent() {
  return <div>User</div>;
}
